//! `good_lp` based ILP extractor.
use super::*;
use good_lp::{
    solvers, Constraint, Expression, IntoAffineExpression, ProblemVariables, Solution, Solver,
    SolverModel, Variable, VariableDefinition,
};
use std::time::Instant;

/// Solver backend to use for ILP extraction.
pub enum IlpSolver {
    #[cfg(feature = "ilp-cbc")]
    CoinCbc,
    #[cfg(feature = "ilp-highs")]
    Highs,
    #[cfg(feature = "ilp-microlp")]
    MicroLp,
    #[cfg(feature = "ilp-scip")]
    Scip,
}

pub struct GoodExtractor {
    pub ilp_solver: IlpSolver,

    /// Solver to provide the initial solution, if any
    pub initial_solution: Option<Box<dyn Extractor>>,
}

impl Extractor for GoodExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        let initial = self
            .initial_solution
            .as_ref()
            .map(|s| s.extract(egraph, roots).choices);
        let problem = construct_problem(egraph, roots, initial);
        match self.ilp_solver {
            #[cfg(feature = "ilp-cbc")]
            IlpSolver::CoinCbc => solve(solvers::coin_cbc::coin_cbc, problem),
            #[cfg(feature = "ilp-highs")]
            IlpSolver::Highs => solve(solvers::highs::highs, problem),
            #[cfg(feature = "ilp-microlp")]
            IlpSolver::MicroLp => solve(solvers::microlp::microlp, problem),
            #[cfg(feature = "ilp-scip")]
            IlpSolver::Scip => solve(solvers::scip::scip, problem),
        }
    }
}

struct IlpProblem {
    vars: ProblemVariables,
    class_active: IndexMap<ClassId, Variable>,
    node_active: IndexMap<(ClassId, NodeId), Variable>,
    objective: Expression,
    constraints: Vec<Constraint>,
}

fn construct_problem(
    egraph: &EGraph,
    roots: &[ClassId],
    initial: Option<IndexMap<ClassId, NodeId>>,
) -> IlpProblem {
    let start = Instant::now();
    let mut vars = ProblemVariables::new();

    // Class active variables
    let class_active = {
        let mut map = IndexMap::new();
        for (cid, _) in egraph.classes().iter() {
            let v = VariableDefinition::new()
                .binary()
                .name(format!("active_{cid}"));
            let v = if let Some(initial) = &initial {
                v.initial(if initial.contains_key(cid) {
                    1.0_f64
                } else {
                    0.0_f64
                })
            } else {
                v
            };
            let v = vars.add(v);
            map.insert(cid.clone(), v);
        }
        map
    };

    // Node active variables
    let node_active = {
        let mut map = IndexMap::new();
        for (cid, class) in egraph.classes().iter() {
            for nid in &class.nodes {
                let v = VariableDefinition::new()
                    .binary()
                    .name(format!("node_{}_{}", cid, nid));
                let v = if let Some(initial) = &initial {
                    v.initial(if initial.get(cid) == Some(nid) {
                        1.0_f64
                    } else {
                        0.0_f64
                    })
                } else {
                    v
                };
                let v = vars.add(v);
                map.insert((cid.clone(), nid.clone()), v);
            }
        }
        map
    };

    // Build the objective
    let mut objective: Expression = 0.0.into();
    for (cid, class) in egraph.classes().iter() {
        for nid in &class.nodes {
            let cost = egraph.nodes[nid].cost.into_inner();
            let var = node_active[&(cid.clone(), nid.clone())];
            objective += cost * var;
        }
    }

    // Construct constraints
    let mut constraints = vec![];

    // Each root must be active
    for root in roots {
        let var = class_active[root];
        constraints.push(var.into_expression().eq(1));
    }
    // If a node is active, its class must be active
    for ((cid, _nid), &node_var) in &node_active {
        let class_var = class_active[cid];
        constraints.push(node_var.into_expression().leq(class_var));
    }
    // If a class is active, exactly one of its nodes must be active
    for (cid, class) in egraph.classes().iter() {
        let class_var = class_active[cid];
        let node_vars: Expression = class
            .nodes
            .iter()
            .map(|nid| node_active[&(cid.clone(), nid.clone())])
            .sum();
        constraints.push(node_vars.eq(class_var));
    }
    // If a node is active, its children must be active
    for ((_cid, nid), &node_var) in &node_active {
        let node = &egraph[nid];
        for child in &node.children {
            let child_cid = egraph.nid_to_cid(child);
            let child_var = class_active[child_cid];
            constraints.push(node_var.into_expression().leq(child_var));
        }
    }

    log::info!(
        "Constructed ILP problem with {} variables and {} constraints in {:?}",
        vars.len(),
        constraints.len(),
        start.elapsed()
    );
    IlpProblem {
        vars,
        class_active,
        node_active,
        objective,
        constraints,
    }
}

fn solve<S: Solver>(solver: S, problem: IlpProblem) -> ExtractionResult {
    let start = Instant::now();
    log::info!("Starting ILP extraction with solver {:?}", S::name());

    let solution = problem
        .vars
        .minimise(problem.objective)
        .using(solver)
        .with_all(problem.constraints)
        .solve()
        .expect("Solving failed.");

    log::info!(
        "Solved ILP in {:?} with status {:?}",
        start.elapsed(),
        solution.status()
    );

    let mut choices = IndexMap::new();
    for ((cid, nid), var) in problem.node_active {
        if solution.value(var).round() as i32 == 1 {
            choices.insert(cid, nid);
        }
    }
    ExtractionResult { choices }
}
