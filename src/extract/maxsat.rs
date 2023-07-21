use crate::{ClassId, EGraph, ExtractionResult, Extractor, Node};
use egraph_serialize::NodeId;
use itertools::Itertools;

use self::cycles::*;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::Instant;

mod cycles {
    use egraph_serialize::NodeId;

    use crate::{ClassId, EGraph, Node, PathBuf};
    use std::collections::{HashMap, HashSet, VecDeque};

    pub struct HyperGraph {
        /// hyper-edges from an e-class to its neighbors
        /// multiple e-nodes might be connecting to the same e-node
        /// therefore, we collapse them together and record the corresponding
        /// e-nodes (as usize, representing the variables in the MAXSAT problem)
        edges: HashMap<usize, HashMap<usize, HashSet<usize>>>,
        nodes: HashSet<usize>,
        ids_to_nodes: HashMap<ClassId, usize>,
        nodes_to_ids: HashMap<usize, ClassId>,
        num_nodes: usize,
    }

    impl HyperGraph {
        pub fn new() -> Self {
            HyperGraph {
                edges: HashMap::new(),
                nodes: HashSet::new(),
                ids_to_nodes: HashMap::new(),
                nodes_to_ids: HashMap::new(),
                num_nodes: 0,
            }
        }

        pub fn contains(&self, eclass: &ClassId) -> bool {
            self.ids_to_nodes.contains_key(eclass)
        }

        pub fn edges(&self, eclass: &ClassId) -> Option<HashMap<ClassId, &HashSet<usize>>> {
            if self.contains(eclass) {
                let mut result = HashMap::new();
                for (to, enodes) in self.edges[&self.ids_to_nodes[eclass]].iter() {
                    result.insert(self.nodes_to_ids[to].clone(), enodes);
                }
                Some(result)
            } else {
                None
            }
        }

        pub fn nodes(&self) -> HashSet<ClassId> {
            self.nodes
                .iter()
                .map(|x| self.nodes_to_ids[x].clone())
                .collect()
        }

        pub fn dump(&self, path: PathBuf) {
            // let f = std::fs::
            let mut graph_str = String::from("");
            for (u, v) in self.edges.iter() {
                for w in v.keys() {
                    graph_str +=
                        &format!("{} {}\n", self.get_id_by_node(*u), self.get_id_by_node(*w));
                }
            }
            std::fs::write(path, graph_str);
        }

        fn add_node(&mut self, k: ClassId) {
            let node_id = self.num_nodes;
            self.ids_to_nodes.insert(k.clone(), node_id);
            self.nodes_to_ids.insert(node_id, k);
            self.edges.insert(node_id, HashMap::new());
            self.nodes.insert(node_id);
            self.num_nodes += 1;
        }

        fn connect(&mut self, from: &ClassId, to: &ClassId, enode: usize) {
            if !self.contains(from) {
                self.add_node(from.clone());
            }
            if !self.contains(to) {
                self.add_node(to.clone());
            }
            let from = &self.ids_to_nodes[from];
            let to = &self.ids_to_nodes[to];
            if !self.edges[from].contains_key(to) {
                self.edges
                    .get_mut(from)
                    .unwrap()
                    .insert(*to, HashSet::from([enode]));
            } else {
                self.edges
                    .get_mut(from)
                    .unwrap()
                    .get_mut(to)
                    .unwrap()
                    .insert(enode);
            }
        }

        pub fn stats(&self) {
            println!("Num Nodes: {}", self.nodes.len());
            println!(
                "Num Edges: {}",
                self.edges.values().map(|m| m.len()).sum::<usize>()
            );
        }

        pub fn neighbors(&self, u: &ClassId) -> Vec<&ClassId> {
            if self.contains(u) {
                self.edges[&self.ids_to_nodes[u]]
                    .keys()
                    .map(|x| &self.nodes_to_ids[x])
                    .collect()
            } else {
                vec![]
            }
        }

        pub fn get_node_by_id(&self, id: &ClassId) -> usize {
            self.ids_to_nodes[id]
        }

        pub fn get_id_by_node(&self, node: usize) -> ClassId {
            self.nodes_to_ids[&node].clone()
        }

        pub fn remove_node_raw(&mut self, node: usize) {
            if self.nodes.contains(&node) {
                self.edges.remove(&node);
                for (k, v) in self.edges.iter_mut() {
                    v.remove(&node);
                }
                self.nodes.remove(&node);
            }
        }

        pub fn remove_node(&mut self, node: &ClassId) {
            let node_id = &self.ids_to_nodes[node];
            if self.contains(node) {
                self.edges.remove(node_id);
                for (k, v) in self.edges.iter_mut() {
                    v.remove(node_id);
                }
                self.nodes.remove(node_id);
            }
        }

        pub fn size(&self) -> usize {
            self.nodes.len()
        }

        pub fn subgraph<'a, T: Iterator<Item = &'a ClassId>>(&self, nodes: T) -> Self {
            let mut graph = HyperGraph::new();
            let node_set: HashSet<&ClassId> = nodes.collect();
            for &n in node_set.iter() {
                assert!(self.contains(n));
                let edges = self.edges(n).unwrap();
                for (neighbor, enodes) in edges.iter() {
                    if !node_set.contains(neighbor) {
                        continue;
                    }
                    for enode in enodes.iter() {
                        graph.connect(n, neighbor, *enode);
                    }
                }
            }
            graph
        }
    }

    pub fn to_hypergraph(
        root: &ClassId,
        egraph: &EGraph,
        node_vars: &HashMap<NodeId, usize>,
        hgraph: &mut HyperGraph,
    ) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_front(root.clone());
        visited.insert(root.clone());
        while !queue.is_empty() {
            let front = queue.pop_front().unwrap();
            for node in egraph.classes()[&front].nodes.iter() {
                for ch in egraph.nodes[node]
                    .children
                    .iter()
                    .map(|x| egraph.nid_to_cid(x))
                {
                    let canonical = ch.clone();
                    hgraph.connect(&front, &canonical, node_vars[node]);
                    if !visited.contains(&canonical) {
                        visited.insert(canonical.clone());
                        queue.push_back(canonical);
                    }
                }
            }
        }
    }

    pub mod scc {
        use itertools::Itertools;

        use super::*;

        fn scc_impl(
            v: &ClassId,
            graph: &HyperGraph,
            num: &mut HashMap<ClassId, usize>,
            low: &mut HashMap<ClassId, usize>,
            stack: &mut Vec<ClassId>,
            visited: &mut HashSet<ClassId>,
            onstack: &mut HashSet<ClassId>,
            idx: &mut usize,
            scc: &mut Vec<Vec<ClassId>>,
        ) {
            num.insert(v.clone(), *idx);
            low.insert(v.clone(), *idx);
            *idx += 1;
            visited.insert(v.clone());
            stack.push(v.clone());
            onstack.insert(v.clone());

            for u in graph.neighbors(v) {
                if !visited.contains(u) {
                    // a tree edge
                    scc_impl(u, graph, num, low, stack, visited, onstack, idx, scc);
                    if low[v] > low[u] {
                        low.insert(v.clone(), low[u]);
                    }
                } else if onstack.contains(u) {
                    // back edge
                    if low[v] > num[u] {
                        low.insert(v.clone(), num[u]);
                    }
                }
            }
            if low[v] == num[v] {
                // found an scc
                let mut scc_found = Vec::new();
                let mut scc_rt = stack.pop().unwrap();
                onstack.remove(&scc_rt);
                while scc_rt != *v {
                    scc_found.push(scc_rt);
                    scc_rt = stack.pop().unwrap();
                    onstack.remove(&scc_rt);
                }
                scc_found.push(scc_rt);
                scc.push(scc_found);
            }
        }

        pub fn scc(graph: &HyperGraph) -> Vec<Vec<ClassId>> {
            let mut num = HashMap::new();
            let mut low = HashMap::new();
            let mut visited = HashSet::new();
            let mut processed = HashSet::new();
            let mut stack = Vec::new();
            let mut idx = 0;
            let mut scc = Vec::new();
            for v in graph.nodes().iter().sorted() {
                if !visited.contains(v) {
                    scc_impl(
                        v,
                        graph,
                        &mut num,
                        &mut low,
                        &mut stack,
                        &mut visited,
                        &mut processed,
                        &mut idx,
                        &mut scc,
                    )
                }
            }
            return scc;
        }
    }

    pub mod johnson {
        use itertools::Itertools;

        use super::*;

        fn unblock(
            v: ClassId,
            blocked: &mut HashSet<ClassId>,
            blocked_map: &mut HashMap<ClassId, HashSet<ClassId>>,
        ) {
            blocked.remove(&v);
            if let Some(blocked_set) = blocked_map.get_mut(&v) {
                let worklist = blocked_set.drain().collect_vec();
                for w in worklist {
                    if blocked.contains(&w) {
                        unblock(w, blocked, blocked_map);
                    }
                }
            }
        }

        fn johnson_alg_impl(
            s: ClassId,
            v: ClassId,
            graph: &HyperGraph,
            blocked: &mut HashSet<ClassId>,
            stack: &mut Vec<ClassId>,
            block_map: &mut HashMap<ClassId, HashSet<ClassId>>,
            cycles: &mut Vec<Vec<ClassId>>,
        ) -> bool {
            let mut f = true;
            blocked.insert(v.clone());
            stack.push(v.clone());
            for w in graph.neighbors(&v) {
                if *w == s {
                    f = true;
                    cycles.push(stack.clone());
                } else if !blocked.contains(w) {
                    f = johnson_alg_impl(
                        s.clone(),
                        w.clone(),
                        graph,
                        blocked,
                        stack,
                        block_map,
                        cycles,
                    ) || f;
                }
            }

            if f {
                unblock(v, blocked, block_map);
            } else {
                for w in graph.neighbors(&v) {
                    if !block_map.contains_key(w) {
                        block_map.insert(w.clone(), HashSet::new());
                    }
                    block_map.get_mut(w).unwrap().insert(v.clone());
                }
            }
            stack.pop();
            f
        }

        pub fn find_cycles(hgraph: &HyperGraph) -> Vec<Vec<ClassId>> {
            let mut scc = scc::scc(hgraph)
                .into_iter()
                .filter(|c| c.len() > 1)
                .collect_vec();
            let mut cycles = Vec::new();
            for n in hgraph.nodes() {
                if hgraph.neighbors(&n).contains(&&n) {
                    cycles.push(vec![n]);
                }
            }
            let mut blocked = HashSet::new();
            let mut block_map = HashMap::new();
            let mut stack = Vec::new();
            while !scc.is_empty() {
                let cur_scc = scc.pop().unwrap();
                let mut subgraph = hgraph.subgraph(cur_scc.iter());
                for i in 0..cur_scc.len() {
                    blocked.clear();
                    block_map.clear();
                    let v = subgraph.get_id_by_node(i);
                    johnson_alg_impl(
                        v.clone(),
                        v,
                        &subgraph,
                        &mut blocked,
                        &mut stack,
                        &mut block_map,
                        &mut cycles,
                    );
                    subgraph.remove_node_raw(i);
                }
            }
            cycles
        }
    }
}

fn tseytin_encoding(clauses: Vec<Vec<usize>>, problem_writer: &mut ProblemWriter, top: f64) {
    let mut var_map = HashMap::new();
    for (i, c) in clauses.iter().enumerate() {
        if c.len() > 1 {
            // new variable to represent the clause
            let v = problem_writer.new_var();
            var_map.insert(i, v);
            // v <-> c
            // == v -> c /\ c -> v
            // == -v \/ c /\ -c \/ v
            // == -v \/ c AND -c \/ v
            // for `c`, it is a conjunction of (negation of) variables therefore
            // 1. -v \/ c == -v \/ -x /\ -v \/ -y /\ -v \/ -z ...
            // -c \/ v == -(-x /\ -y /\ -z ...) \/ v
            // 2. == x \/ y \/ z \/ ... \/ v

            // Add 1 as hard clauses
            for x in c {
                problem_writer.hard_clause(&format!("-{} -{}", v, x), top);
            }
            // Add 2 as hard clauses
            problem_writer.hard_clause(
                &format!(
                    "{} {}",
                    c.iter()
                        .map(|x| format!("{}", x))
                        .collect::<Vec<_>>()
                        .join(" "),
                    v
                ),
                top,
            );
        }
    }
    // Finally, tseytin encoding for the clauses
    // == v1 \/ v2 \/ ... \/ vn
    problem_writer.hard_clause(
        &clauses
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if c.len() > 1 {
                    format!("{}", var_map[&i])
                } else {
                    format!("-{}", c[0])
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        top,
    );
}

#[derive(Clone)]
struct ProblemWriter {
    pub path: String,
    problem: String,
    parameters: String,
    clause_counter: usize,
    var_counter: usize,
}

impl ProblemWriter {
    pub fn new(path: String) -> Self {
        Self {
            path,
            problem: String::new(),
            parameters: String::new(),
            clause_counter: 0,
            var_counter: 0,
        }
    }

    pub fn new_var(&mut self) -> usize {
        self.var_counter += 1;
        self.var_counter
    }

    pub fn comment(&mut self, comment: &str) {
        self.problem.push_str(&format!("c {}\n", comment));
    }

    pub fn parameters(&mut self, top: f64) {
        self.parameters = format!(
            "p wcnf {} {} {}\n",
            self.var_counter, self.clause_counter, top as i64
        );
    }

    pub fn hard_clause(&mut self, clause: &str, top: f64) {
        self.clause_counter += 1;
        self.problem
            .push_str(&format!("{} {} 0\n", top as i64, clause));
    }

    pub fn soft_clause(&mut self, clause: &str, weight: f64) {
        self.clause_counter += 1;
        self.problem
            .push_str(&format!("{} {} 0\n", weight as i64, clause));
    }

    pub fn dump(&mut self) {
        println!("written to {}", self.path);
        std::fs::write(
            self.path.clone(),
            format!("{}{}", self.parameters.clone(), self.problem.clone()),
        )
        .unwrap();
    }
}

/// the Extractor that constructs the constraint problem
struct MaxsatExtractorImpl<'a> {
    /// EGraph to extract
    pub egraph: &'a EGraph,
    writer: ProblemWriter,
}

/// A weighted partial maxsat problem
struct WeightedPartialMaxsatProblem<'a> {
    // pub class_vars: HashMap<Id, i32>,
    /// a map from enodes to maxsat variables (starting from 1)
    pub node_vars: HashMap<NodeId, usize>,
    /// root eclass Id
    pub roots: Vec<ClassId>,
    /// EGraph to extract
    pub egraph: &'a EGraph,
    top: f64,
    problem_writer: ProblemWriter,
}

impl<'a> WeightedPartialMaxsatProblem<'a> {
    /// Given a weighted partial maxsat problem, solve the problem
    /// and parse the output
    pub fn solve(&self) -> (u128, Option<f64>, ExtractionResult) {
        // assume maxhs installed
        let start = Instant::now();
        let result = Command::new("maxhs")
            .arg("-printSoln")
            .arg(self.problem_writer.path.clone())
            .output();
        let elapsed = start.elapsed().as_millis();
        if let Ok(output) = result {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines = stdout.lines();
            let (mut comments, mut opt_line, mut sol_line, mut solution) =
                (vec![], vec![], vec![], vec![]);
            for l in lines {
                let mut line = l.split(" ");
                if let Some(indicator) = line.next() {
                    match indicator {
                        "c" => comments.push(line.collect::<Vec<_>>().join(" ")),
                        "o" => opt_line.push(line.collect::<Vec<_>>().join(" ")),
                        "s" => sol_line.push(line.collect::<Vec<_>>().join(" ")),
                        "v" => solution.push(line.collect::<Vec<_>>().join(" ")),
                        _ => (),
                    }
                }
            }
            assert!(sol_line.len() > 0, "Solution cannot be empty");
            let sol = sol_line.iter().next().unwrap();
            if sol.contains("UNSATISFIABLE") {
                panic!("Problem UNSAT")
            } else {
                assert!(
                    solution.len() > 0,
                    "No solution line (try add -printSoln option to maxhs)"
                );
                let sol = solution.iter().next().unwrap();
                let sat_map = sol
                    .chars()
                    .enumerate()
                    .filter(|(_, res)| *res == '1')
                    .map(|(car, _)| car + 1)
                    .collect::<HashSet<_>>();
                let mut worklist = Vec::new();
                let mut selected = HashSet::new();
                worklist.extend(self.roots.clone());
                let mut result = ExtractionResult::default();
                while let Some(id) = worklist.last() {
                    let id = id.clone();
                    if selected.contains(&id) {
                        worklist.pop();
                        continue;
                    }
                    let mut not_found = true;
                    for (_, n) in self.egraph.classes()[&id].nodes.iter().enumerate() {
                        if sat_map.contains(&self.node_vars[&n]) {
                            not_found = false;
                            if self.egraph.nodes[n]
                                .children
                                .iter()
                                .all(|ch| selected.contains(self.egraph.nid_to_cid(ch)))
                            {
                                // result.choices[id] = i;
                                result.choose(id.clone(), n.clone());
                                selected.insert(id.clone());
                                worklist.pop();
                            } else {
                                worklist.extend_from_slice(
                                    self.egraph.nodes[n]
                                        .children
                                        .iter()
                                        .map(|x| self.egraph.nid_to_cid(x))
                                        .cloned()
                                        .collect::<Vec<_>>()
                                        .as_slice(),
                                );
                            }
                            break;
                        }
                    }
                    if not_found {
                        panic!("No active node for eclass: {}", id.clone());
                    }
                }
                // parse opt
                if opt_line.len() > 0 {
                    let opt = opt_line.iter().next().unwrap();
                    return (elapsed, Some(opt.parse::<f64>().unwrap()), result);
                }
                return (elapsed, None, result);
            }
        } else {
            panic!(
                "Unable to solve {}, err: {}",
                self.problem_writer.path,
                result.err().unwrap()
            );
        }
    }
}

impl<'a> MaxsatExtractorImpl<'a> {
    /// create a new maxsat extractor
    pub fn new(egraph: &'a EGraph, path: String) -> Self {
        Self {
            egraph,
            writer: ProblemWriter::new(path.clone()),
        }
    }

    /// create a maxsat problem
    pub fn create_problem(
        &mut self,
        roots: Vec<ClassId>,
        name: &str,
        no_cycle: bool,
    ) -> WeightedPartialMaxsatProblem<'a> {
        // Hard Constraints
        // === root constraint (pick at least one in root)
        // \forall n \in R, \bigvee v_n
        // === children constraint
        // \forall n, \forall C\in children(n), v_n -> \bigvee_cN v_cN \forall cN \in C
        self.writer.comment(&format!("Problem: {}", name));
        // create variables
        let mut top = 0 as f64;
        let mut node_vars = HashMap::default();
        let mut node_weight_map = HashMap::new();
        for (_, c) in self.egraph.classes().iter() {
            for n in c.nodes.iter() {
                node_vars.insert(n.clone(), self.writer.new_var());

                node_weight_map.insert(n.clone(), self.egraph[n].cost);
                top += f64::from(self.egraph[n].cost);
            }
        }

        let top = top + 1 as f64;

        // Hard clauses
        let mut hard_clauses = Vec::new();
        // root constraint
        for root in roots.iter() {
            let root_clause = self.egraph.classes()[root]
                .nodes
                .iter()
                .map(|n| node_vars[n])
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            hard_clauses.push(root_clause);
        }

        let mut node_to_children = HashMap::new();
        // children constraint
        for (_, c) in self.egraph.classes().iter() {
            for n in c.nodes.iter() {
                // v_n -> \bigvee_cN v_cN forall C
                let mut node_children = HashSet::new();
                for ch in self.egraph.nodes[n]
                    .children
                    .iter()
                    .map(|x| self.egraph.nid_to_cid(x))
                {
                    node_children.insert(ch.clone());
                    let mut clause = String::new();
                    clause.push_str(&format!("-{}", node_vars[n]));
                    for ch_node in self.egraph.classes()[ch].nodes.iter() {
                        clause.push_str(&format!(" {}", node_vars[ch_node]));
                    }
                    hard_clauses.push(clause);
                }
                node_to_children.insert(node_vars[n], node_children);
            }
        }

        // cycle constraint
        if no_cycle {
            let mut hgraph = HyperGraph::new();
            for root in roots.iter() {
                to_hypergraph(root, &self.egraph, &node_vars, &mut hgraph);
            }
            let class_cycles = cycles::johnson::find_cycles(&hgraph);
            for c in class_cycles {
                if c.len() == 1 {
                    for n in self.egraph.classes()[&c[0]].nodes.iter() {
                        if self.egraph.nodes[n]
                            .children
                            .iter()
                            .map(|x| self.egraph.nid_to_cid(x))
                            .contains(&c[0])
                        {
                            self.writer.hard_clause(&format!("-{}", node_vars[n]), top);
                        }
                    }
                } else {
                    let mut clauses = Vec::new();
                    for i in 0..c.len() {
                        let next_hop = (i + 1) % c.len();
                        let u = hgraph.edges(&c[i]).unwrap();
                        let v = u[&c[next_hop]].clone();
                        clauses.push(v.into_iter().collect::<Vec<_>>());
                    }
                    tseytin_encoding(clauses, &mut self.writer, top);
                }
            }
        }

        // soft clauses (i.e. not all nodes need to be picked)
        let mut soft_clauses = HashMap::new();
        for (_, c) in self.egraph.classes().iter() {
            for n in c.nodes.iter() {
                soft_clauses.insert(n.clone(), format!("-{}", node_vars[n]));
            }
        }

        self.writer.comment("Hard clauses:");
        for clause in hard_clauses {
            self.writer.hard_clause(&clause, top);
        }

        self.writer.comment("Soft clauses:");
        for (n, clause) in soft_clauses {
            self.writer
                .soft_clause(&clause, f64::from(node_weight_map[&n]));
        }

        self.writer.parameters(top);

        self.writer.dump();

        WeightedPartialMaxsatProblem {
            top,
            node_vars,
            roots,
            egraph: self.egraph,
            problem_writer: self.writer.clone(),
        }
    }
}

fn maxsat_extract(egraph: &EGraph, path: String, roots: Vec<ClassId>) -> ExtractionResult {
    let mut extractor = MaxsatExtractorImpl::new(egraph, path);
    let problem = extractor.create_problem(roots, "maxsat_ext", true);
    problem.solve().2
}

pub struct MaxsatExtractor;

impl Extractor for MaxsatExtractor {
    fn extract(&self, egraph: &EGraph, roots: &[ClassId]) -> ExtractionResult {
        maxsat_extract(egraph, "maxsat_extract.txt".into(), roots.to_vec())
    }
}
