/* Uses COIN-OR CBC solver to find an extraction from the egraph where each node is only costed once.

Some parts of the graph are easy to solve, for example tree parts, which can be collapsed down to a single class before the solver is called.

The solver is called repeatedly, with the cycles found in the previous solution blocked.

*/

use super::*;
use coin_cbc::{Col, Model};
use indexmap::IndexSet;
use std::fmt;
use std::time::SystemTime;

const PULL_UP_COSTS: bool = true;
const REMOVE_SELF_LOOPS: bool = true;
const REMOVE_HIGH_COST_NODES: bool = true;
const REMOVE_MORE_EXPENSIVE_SUBSUMED_NODES: bool = true;
const REMOVE_MORE_EXPENSIVE_NODES: bool = true;
const REMOVE_UNREACHABLE_CLASSES: bool = true;
const PULL_UP_SINGLE_PARENT: bool = true;

const MOVE_MIN_COST_OF_MEMBERS_TO_CLASS: bool = false;
const INITIALISE_WITH_APPROX: bool = false;
const INITIALISE_WITH_PREVIOUS_SOLUTION: bool = false;

// Some problems take >10 hours to optimise.
const SOLVING_TIME_LIMIT_SECONDS: u64 = 10;

struct NodeILP {
    variable: Col,
    cost: Cost,
    member: NodeId,
    children_classes: IndexSet<ClassId>,
}

struct ClassILP {
    active: Col,
    members: Vec<NodeId>,
    variables: Vec<Col>,
    costs: Vec<Cost>,

    // Initially this contains the children of each member (respectively), but
    // gets edited during the run, so mightn't match later on.
    childrens_classes: Vec<IndexSet<ClassId>>,
}

impl fmt::Debug for ClassILP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "classILP[{}] {{ node: {:?}, children: {:?},  cost: {:?} }}",
            self.members(),
            self.members,
            self.childrens_classes,
            self.costs
        )
    }
}

impl ClassILP {
    fn remove(&mut self, idx: usize) {
        self.variables.remove(idx);
        self.costs.remove(idx);
        self.members.remove(idx);
        self.childrens_classes.remove(idx);
    }

    fn remove_node(&mut self, node_id: &NodeId) {
        if let Some(idx) = self.members.iter().position(|n| n == node_id) {
            self.remove(idx);
        }
    }

    fn members(&self) -> usize {
        return self.variables.len();
    }

    fn check(&self) {
        assert_eq!(self.variables.len(), self.costs.len());
        assert_eq!(self.variables.len(), self.members.len());
        assert_eq!(self.variables.len(), self.childrens_classes.len());
    }

    fn as_nodes(&self) -> Vec<NodeILP> {
        self.variables
            .iter()
            .zip(&self.costs)
            .zip(&self.members)
            .zip(&self.childrens_classes)
            .map(|(((variable, &cost_), member), children_classes)| NodeILP {
                variable: *variable,
                cost: cost_,
                member: member.clone(),
                children_classes: children_classes.clone(),
            })
            .collect()
    }

    fn get_children_of_node(&self, node_id: &NodeId) -> &IndexSet<ClassId> {
        let idx = self.members.iter().position(|n| n == node_id).unwrap();
        return &self.childrens_classes[idx];
    }

    fn get_variable_for_node(&self, node_id: &NodeId) -> Col {
        let idx = self.members.iter().position(|n| n == node_id).unwrap();
        return self.variables[idx].clone();
    }
}

pub struct CbcExtractor;

impl Extractor for CbcExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        let mut model = Model::default();

        let true_literal = model.add_binary();
        model.set_col_lower(true_literal, 1.0);

        let false_literal = model.add_binary();
        model.set_col_upper(false_literal, 0.0);

        let n2c = |nid: &NodeId| egraph.nid_to_cid(nid);

        let mut vars: IndexMap<ClassId, ClassILP> = egraph
            .classes()
            .values()
            .map(|class| {
                let cvars = ClassILP {
                    active: model.add_binary(),
                    variables: class.nodes.iter().map(|_| model.add_binary()).collect(),
                    costs: class.nodes.iter().map(|n| egraph[n].cost).collect(),
                    members: class.nodes.clone(),
                    childrens_classes: class
                        .nodes
                        .iter()
                        .map(|n| {
                            egraph[n]
                                .children
                                .iter()
                                .map(|c| n2c(c).clone())
                                .collect::<IndexSet<ClassId>>()
                        })
                        .collect(),
                };
                (class.id.clone(), cvars)
            })
            .collect();

        let initial_result =
            super::faster_greedy_dag::FasterGreedyDagExtractor.extract(egraph, roots);
        let initial_result_cost = initial_result.dag_cost(egraph, roots);

        for _i in 1..3 {
            remove_with_loops(&mut vars, roots);
            remove_high_cost(&mut vars, initial_result_cost);
            remove_more_expensive_nodes(&mut vars, &initial_result, egraph);
            remove_more_expensive_subsumed_nodes(&mut vars);
            remove_unreachable_classes(&mut vars, roots);
            pull_up_with_single_parent(&mut vars, roots);
            pull_up_costs(&mut vars, roots);
        }

        let mut empty = 0;
        for class in vars.values() {
            if class.members() == 0 {
                empty += 1;
            }
        }
        //No problems yet that justify removing these.
        log::info!("Empty classes: {empty}");

        for class in vars.values() {
            if class.members() == 0 {
                model.set_col_upper(class.active, 0.0);
                continue;
            }
            assert!(class.active != false_literal);

            // class active == some node active
            // sum(for node_active in class) == class_active

            let row = model.add_row();
            model.set_row_equal(row, 0.0);
            model.set_weight(row, class.active, -1.0);
            for &node_active in &class.variables.iter().collect::<IndexSet<_>>() {
                model.set_weight(row, *node_active, 1.0);
            }

            let childrens_classes_var =
                |cc: &IndexSet<ClassId>| cc.iter().map(|n| vars[n].active).collect::<IndexSet<_>>();

            let mut intersection: IndexSet<Col> =
                childrens_classes_var(&class.childrens_classes[0].clone());

            for childrens_classes in &class.childrens_classes[1..] {
                intersection = intersection
                    .intersection(&childrens_classes_var(childrens_classes))
                    .cloned()
                    .collect();
            }

            // A class being active implies that all in the intersection
            // of it's children are too.
            for c in &intersection {
                let row = model.add_row();
                model.set_row_upper(row, 0.0);
                model.set_weight(row, class.active, 1.0);
                model.set_weight(row, *c, -1.0);
            }

            for (childrens_classes, &node_active) in
                class.childrens_classes.iter().zip(&class.variables)
            {
                for child_active in childrens_classes_var(childrens_classes) {
                    // node active implies child active, encoded as:
                    //   node_active <= child_active
                    //   node_active - child_active <= 0
                    if !intersection.contains(&child_active) {
                        let row = model.add_row();
                        model.set_row_upper(row, 0.0);
                        model.set_weight(row, node_active, 1.0);
                        model.set_weight(row, child_active, -1.0);
                    }
                }
            }
        }

        for root in roots {
            model.set_col_lower(vars[root].active, 1.0);
        }

        let mut objective_fn_terms = 0;

        for (_class_id, c_var) in &vars {
            let mut min_cost = 0.0;

            /* Moves the minimum of all the nodes up onto the class.
            Most helpful when the members of the class all have the same cost.
            For example if the members' costs are [1,1,1], three terms get
            replaced by one in the objective function.
            */

            if MOVE_MIN_COST_OF_MEMBERS_TO_CLASS {
                min_cost = c_var
                    .costs
                    .iter()
                    .min()
                    .unwrap_or(&Cost::default())
                    .into_inner();
            }

            if min_cost != 0.0 {
                model.set_obj_coeff(c_var.active, min_cost);
                objective_fn_terms += 1;
            }

            for (&node_active, &node_cost) in c_var.variables.iter().zip(c_var.costs.iter()) {
                if *node_cost - min_cost != 0.0 {
                    model.set_obj_coeff(node_active, *node_cost - min_cost);
                }
            }
        }

        log::info!("Objective function terms: {}", objective_fn_terms);

        // set initial solution based on a non-optimal extraction.
        if INITIALISE_WITH_APPROX {
            set_initial_solution(&vars, &mut model, &initial_result);
        }

        if false {
            return initial_result;
        }

        let start_time = SystemTime::now();
        loop {
            // Set the solver limit based on how long has passed already.
            if let Ok(difference) = SystemTime::now().duration_since(start_time) {
                let seconds = SOLVING_TIME_LIMIT_SECONDS.saturating_sub(difference.as_secs());
                model.set_parameter("seconds", &seconds.to_string());
            } else {
                model.set_parameter("seconds", "0");
            }

            //This starts from scratch solving each time. I've looked quickly
            //at the API and didn't see how to call it incrementally.
            let solution = model.solve();
            log::info!(
                "CBC status {:?}, {:?}, obj = {}",
                solution.raw().status(),
                solution.raw().secondary_status(),
                solution.raw().obj_value(),
            );

            if solution.raw().status() != coin_cbc::raw::Status::Finished {
                /* The solver keeps the best discovered feasible solution
                (somewhere). It'd be better to extract that, test if
                it has cycles, and if not return that. */
                log::info!(
                    "Timed out, returning initial solution of: {} ",
                    initial_result_cost.into_inner()
                );
                return initial_result;
            }

            let mut result = ExtractionResult::default();

            let mut cost = 0.0;
            for (id, var) in &vars {
                let active = solution.col(var.active) > 0.0;
                if active {
                    assert_eq!(
                        1,
                        var.variables
                            .iter()
                            .filter(|&n| solution.col(*n) > 0.0)
                            .count()
                    );

                    let node_idx = var
                        .variables
                        .iter()
                        .position(|&n| solution.col(n) > 0.0)
                        .unwrap();
                    let node_id = var.members[node_idx].clone();
                    cost += var.costs[node_idx].into_inner();
                    result.choose(id.clone(), node_id);
                }
            }

            let cycles = find_cycles_in_result(&result, &vars, roots);
            if cycles.is_empty() {
                assert!(cost <= initial_result_cost.into_inner());
                assert_eq!(result.dag_cost(egraph, roots), cost);
                assert_eq!(cost as i64, solution.raw().obj_value() as i64);

                return result;
            } else {
                log::info!("Refining by blocking cycles: {}", cycles.len());
                for c in &cycles {
                    block_cycle(&mut model, c, &vars);
                }
            }

            if INITIALISE_WITH_PREVIOUS_SOLUTION {
                set_initial_solution(&vars, &mut model, &result);
            }
        }
    }
}

fn set_initial_solution(
    vars: &IndexMap<ClassId, ClassILP>,
    model: &mut Model,
    initial_result: &ExtractionResult,
) {
    for (class, class_vars) in vars {
        for col in &class_vars.variables {
            model.set_col_initial_solution(*col, 0.0);
        }

        if let Some(node_id) = initial_result.choices.get(class) {
            model.set_col_initial_solution(class_vars.active, 1.0);
            model.set_col_initial_solution(vars[class].get_variable_for_node(node_id), 1.0);
        } else {
            model.set_col_initial_solution(class_vars.active, 0.0);
        }
    }
}

/*
If the cost of a node, including the full cost of all it's children, is less than the cost of just the other node's (excluding its children)
Then discard the more expensive node.

* The cheapest cost doesn't use the var[] cost, it uses the cost from the egraphs. This is worse, but having dag_cost already
built, makes this super easy to implement.
* This can reduce the number of valid extractions - it will drop nodes that have the same cost as other nodes.
*/

fn remove_more_expensive_nodes(
    vars: &mut IndexMap<ClassId, ClassILP>,
    initial_result: &ExtractionResult,
    egraph: &EGraph,
) {
    if REMOVE_MORE_EXPENSIVE_NODES {
        let mut removed = 0;
        for class in vars.values_mut() {
            let children = class.as_nodes();
            if children.len() <= 2 {
                continue;
            }

            let (cheapest_node, cheapest_cost) = children
                .iter()
                .map(|node| {
                    let cost = initial_result.dag_cost(
                        egraph,
                        node.children_classes
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .as_slice(),
                    ) + egraph[&node.member].cost;
                    (node, cost)
                })
                .min_by_key(|&(_, cost)| cost)
                .unwrap();

            removed += children
                .iter()
                .filter(|e| e.cost >= cheapest_cost && (cheapest_node.member != e.member))
                .map(|e| class.remove_node(&e.member))
                .count();
        }

        log::info!(
            "Removed nodes that are not cheaper than another in the same class: {}",
            removed
        );
    }
}

/* If a node in a class has (a) lower cost than another in the same class, and (b) it's
  children are a subset of the other's, then it can be removed.
*/
fn remove_more_expensive_subsumed_nodes(vars: &mut IndexMap<ClassId, ClassILP>) {
    if REMOVE_MORE_EXPENSIVE_SUBSUMED_NODES {
        let mut removed = 0;
        for class in vars.values_mut() {
            let mut children = class.as_nodes();
            children.sort_by_key(|e| e.children_classes.len());

            let mut to_remove: IndexSet<NodeId> = Default::default();

            for i in 0..children.len() {
                let node_a = &children[i];
                if to_remove.contains(&node_a.member.clone()) {
                    continue;
                }

                for j in (i + 1)..children.len() {
                    let node_b = &children[j];

                    // This removes some extractions with the same cost.
                    if node_a.cost <= node_b.cost
                        && node_a.children_classes.is_subset(&node_b.children_classes)
                    {
                        to_remove.insert(node_b.member.clone());
                    }
                }
            }
            removed += to_remove
                .iter()
                .map(|node_id| class.remove_node(node_id))
                .count();
        }

        log::info!("Removed more expensive subsumed nodes: {}", removed);
    }
}

// Remove any classes that can't be reached from a root.
fn remove_unreachable_classes(vars: &mut IndexMap<ClassId, ClassILP>, roots: &[ClassId]) {
    if REMOVE_UNREACHABLE_CLASSES {
        let mut reachable_classes: IndexSet<ClassId> = IndexSet::default();
        reachable(&*vars, roots, &mut reachable_classes);
        let initial_size = vars.len();
        vars.retain(|class_id, _| reachable_classes.contains(class_id));
        log::info!("Unreachable classes: {}", initial_size - vars.len());
    }
}

/*
For each class with one parent, move the minimum costs of the members to each node in the parent that points to it.

if we iterated through these in order, from child to parent, to parent, to parent.. it could be done in one pass.
*/
fn pull_up_costs(vars: &mut IndexMap<ClassId, ClassILP>, roots: &[ClassId]) {
    if PULL_UP_COSTS {
        let child_to_parent = classes_with_single_parent(&*vars);
        log::info!("Classes with a single parent: {}", child_to_parent.len());

        let mut count = 0;
        let mut changed = true;
        while (count < 10) && changed {
            changed = false;
            for (child, parent) in &child_to_parent {
                count = count + 1;

                if child == parent {
                    continue;
                }
                if roots.contains(child) {
                    continue;
                }

                vars[child].check();
                vars[parent].check();

                // Get the minimum cost of members of the children
                let min_cost = vars[child]
                    .costs
                    .iter()
                    .min()
                    .unwrap_or(&Cost::default())
                    .into_inner();

                assert!(min_cost >= 0.0);
                if min_cost == 0.0 {
                    continue;
                }
                changed = true;

                // Now remove it from each member
                for c in &mut vars[child].costs {
                    *c -= min_cost;
                    assert!(c.into_inner() >= 0.0);
                }
                // Add it onto each node in the parent that refers to this class.
                let indices: Vec<_> = vars[parent]
                    .childrens_classes
                    .iter()
                    .enumerate()
                    .filter(|&(_, c)| c.contains(child))
                    .map(|(id, _)| id)
                    .collect();

                assert!(indices.len() > 0);

                for id in indices {
                    vars[parent].costs[id] += min_cost;
                }
            }
        }
    }
}

/* If a class has a single parent class,
then move the children from the child to the parent class.

There could be a long chain of single parent classes - which this handles
(badly) by looping through a few times.

*/

fn pull_up_with_single_parent(vars: &mut IndexMap<ClassId, ClassILP>, roots: &[ClassId]) {
    if PULL_UP_SINGLE_PARENT {
        for _i in 0..10 {
            let child_to_parent = classes_with_single_parent(&*vars);
            log::info!("Classes with a single parent: {}", child_to_parent.len());

            let mut pull_up_count = 0;
            for (child, parent) in &child_to_parent {
                if child == parent {
                    continue;
                }

                if roots.contains(child) {
                    continue;
                }

                if vars[child].members.len() != 1 {
                    continue;
                }

                if vars[child].childrens_classes.first().unwrap().is_empty() {
                    continue;
                }

                let found = vars[parent]
                    .childrens_classes
                    .iter()
                    .filter(|c| c.contains(child))
                    .count();

                if found != 1 {
                    continue;
                }

                let idx = vars[parent]
                    .childrens_classes
                    .iter()
                    .position(|e| e.contains(child))
                    .unwrap();

                let child_descendants = vars
                    .get(child)
                    .unwrap()
                    .childrens_classes
                    .first()
                    .unwrap()
                    .clone();

                let parent_descendants: &mut IndexSet<ClassId> = vars
                    .get_mut(parent)
                    .unwrap()
                    .childrens_classes
                    .get_mut(idx)
                    .unwrap();

                for e in &child_descendants {
                    parent_descendants.insert(e.clone());
                }

                vars.get_mut(child)
                    .unwrap()
                    .childrens_classes
                    .first_mut()
                    .unwrap()
                    .clear();

                pull_up_count += 1;
            }
            log::info!("Pull up count: {pull_up_count}");
            if pull_up_count == 0 {
                break;
            }
        }
    }
}

// Remove any nodes that alone cost more than the whole best solution.
fn remove_high_cost(vars: &mut IndexMap<ClassId, ClassILP>, initial_result_cost: NotNan<f64>) {
    if REMOVE_HIGH_COST_NODES {
        let mut high_cost = 0;

        for (_class_id, class_details) in vars.iter_mut() {
            let mut to_remove = std::collections::BTreeSet::new();
            for (node_idx, cost) in class_details.costs.iter().enumerate() {
                if cost > &initial_result_cost {
                    to_remove.insert(node_idx);
                }
            }
            for &index in to_remove.iter().rev() {
                class_details.remove(index);
                high_cost += 1;
            }
        }
        log::info!("Omitted high-cost nodes: {}", high_cost);
    }
}

// Remove nodes with any (a) child pointing back to its own class,
// or (b) any child pointing to the sole root class.
fn remove_with_loops(vars: &mut IndexMap<ClassId, ClassILP>, roots: &[ClassId]) {
    if REMOVE_SELF_LOOPS {
        let mut self_loop = 0;
        for (class_id, class_details) in vars.iter_mut() {
            let mut to_remove = std::collections::BTreeSet::new();
            for (node_idx, children) in class_details.childrens_classes.iter().enumerate() {
                if children
                    .iter()
                    .any(|cid| *cid == *class_id || (roots.len() == 1 && roots[0] == *cid))
                {
                    to_remove.insert(node_idx);
                }
            }

            for &index in to_remove.iter().rev() {
                class_details.remove(index);
                self_loop += 1;
            }
        }

        log::info!("Omitted looping nodes: {}", self_loop);
    }
}

// Mapping from child class to parent classes
fn classes_with_single_parent(vars: &IndexMap<ClassId, ClassILP>) -> IndexMap<ClassId, ClassId> {
    let mut child_to_parents: IndexMap<ClassId, IndexSet<ClassId>> = IndexMap::new();

    for (class_id, class_vars) in vars.iter() {
        for kids in &class_vars.childrens_classes {
            for child_class in kids {
                child_to_parents
                    .entry(child_class.clone())
                    .or_insert_with(IndexSet::new)
                    .insert(class_id.clone());
            }
        }
    }

    // return classes with only one parent
    child_to_parents
        .into_iter()
        .filter_map(|(child_class, parents)| {
            if parents.len() == 1 {
                Some((child_class, parents.into_iter().next().unwrap()))
            } else {
                None
            }
        })
        .collect()
}

//Set of classes that can be reached from the [classes]
fn reachable(
    vars: &IndexMap<ClassId, ClassILP>,
    classes: &[ClassId],
    is_reachable: &mut IndexSet<ClassId>,
) {
    for class in classes {
        if is_reachable.insert(class.clone()) {
            let class_vars = vars.get(class).unwrap();
            for kids in &class_vars.childrens_classes {
                for child_class in kids {
                    reachable(vars, &[child_class.clone()], is_reachable);
                }
            }
        }
    }
}

// Adds constraints to stop the cycle.
fn block_cycle(model: &mut Model, cycle: &Vec<ClassId>, vars: &IndexMap<ClassId, ClassILP>) {
    if cycle.is_empty() {
        return;
    }
    let mut blocking = Vec::new();
    for i in 0..cycle.len() {
        let current_class_id = &cycle[i];
        let next_class_id = &cycle[(i + 1) % cycle.len()];

        let blocking_var = model.add_binary();
        blocking.push(blocking_var);
        for node in &vars[current_class_id].as_nodes() {
            if node.children_classes.contains(next_class_id) {
                let row = model.add_row();
                model.set_row_upper(row, 0.0);
                model.set_weight(row, node.variable, 1.0);
                model.set_weight(row, blocking_var, -1.0);
            }
        }
    }

    //One of the edges between nodes in the cycle shouldn't be activated:
    let row = model.add_row();
    model.set_row_upper(row, blocking.len() as f64 - 1.0);
    for b in blocking {
        model.set_weight(row, b, 1.0)
    }
}

#[derive(Clone)]
enum TraverseStatus {
    Doing,
    Done,
}

/*
Returns the simple cycles possible from the roots.

Because the number of simple cycles can be factorial in the number
of nodes, this can be very slow.

Imagine a 20 node complete graph with one root. From the first node you have
19 choices, then from the second 18 choices, etc.  When you get to the second
last node you go back to the root. There are about 10^17 length 18 cycles.

So we limit how many can be found.
*/
const CYCLE_LIMIT: usize = 1000;

fn find_cycles_in_result(
    extraction_result: &ExtractionResult,
    vars: &IndexMap<ClassId, ClassILP>,
    roots: &[ClassId],
) -> Vec<Vec<ClassId>> {
    let mut status = IndexMap::<ClassId, TraverseStatus>::default();
    let mut cycles = vec![];
    for root in roots {
        let mut stack = vec![];
        cycle_dfs(
            extraction_result,
            vars,
            root,
            &mut status,
            &mut cycles,
            &mut stack,
        )
    }
    cycles
}

fn cycle_dfs(
    extraction_result: &ExtractionResult,
    vars: &IndexMap<ClassId, ClassILP>,
    class_id: &ClassId,
    status: &mut IndexMap<ClassId, TraverseStatus>,
    cycles: &mut Vec<Vec<ClassId>>,
    stack: &mut Vec<ClassId>,
) {
    match status.get(class_id).cloned() {
        Some(TraverseStatus::Done) => (),
        Some(TraverseStatus::Doing) => {
            // Get the part of the stack between the first visit to the class and now.
            let mut cycle = vec![];
            if let Some(pos) = stack.iter().position(|id| id == class_id) {
                cycle.extend_from_slice(&stack[pos..]);
            }
            cycles.push(cycle);
        }
        None => {
            if cycles.len() > CYCLE_LIMIT {
                return;
            }
            status.insert(class_id.clone(), TraverseStatus::Doing);
            stack.push(class_id.clone());
            let node_id = &extraction_result.choices[class_id];
            for child_cid in vars[class_id].get_children_of_node(node_id) {
                cycle_dfs(extraction_result, vars, child_cid, status, cycles, stack)
            }
            let last = stack.pop();
            assert_eq!(*class_id, last.unwrap());
            status.insert(class_id.clone(), TraverseStatus::Done);
        }
    }
}
