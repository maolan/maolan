use std::collections::HashSet;
use std::hash::Hash;

pub fn would_create_cycle<Node, Neighbors, Iter>(
    from: &Node,
    to: &Node,
    mut neighbors: Neighbors,
) -> bool
where
    Node: Clone + Eq + Hash,
    Neighbors: FnMut(&Node) -> Iter,
    Iter: IntoIterator<Item = Node>,
{
    if from == to {
        return true;
    }
    has_path(to, from, &mut neighbors)
}

fn has_path<Node, Neighbors, Iter>(start: &Node, target: &Node, neighbors: &mut Neighbors) -> bool
where
    Node: Clone + Eq + Hash,
    Neighbors: FnMut(&Node) -> Iter,
    Iter: IntoIterator<Item = Node>,
{
    let mut visited = HashSet::new();
    has_path_inner(start, target, neighbors, &mut visited)
}

fn has_path_inner<Node, Neighbors, Iter>(
    current: &Node,
    target: &Node,
    neighbors: &mut Neighbors,
    visited: &mut HashSet<Node>,
) -> bool
where
    Node: Clone + Eq + Hash,
    Neighbors: FnMut(&Node) -> Iter,
    Iter: IntoIterator<Item = Node>,
{
    if current == target {
        return true;
    }
    if !visited.insert(current.clone()) {
        return false;
    }
    for next in neighbors(current) {
        if has_path_inner(&next, target, neighbors, visited) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::would_create_cycle;
    use std::collections::HashMap;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Node {
        TrackIn,
        TrackOut,
        PluginA,
        PluginB,
    }

    #[test]
    fn detects_track_cycle() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec!["C".to_string()]),
            ("C".to_string(), vec![]),
        ]);

        let from = "C".to_string();
        let to = "A".to_string();
        assert!(would_create_cycle(&from, &to, |node: &String| {
            graph.get(node).cloned().unwrap_or_default()
        }));
    }

    #[test]
    fn detects_plugin_cycle() {
        let graph = HashMap::from([
            (Node::TrackIn, vec![Node::PluginA]),
            (Node::PluginA, vec![Node::PluginB]),
            (Node::PluginB, vec![Node::TrackOut]),
            (Node::TrackOut, vec![]),
        ]);

        assert!(would_create_cycle(&Node::PluginB, &Node::PluginA, |node| {
            graph.get(node).cloned().unwrap_or_default()
        }));
    }

    #[test]
    fn allows_acyclic_edge() {
        let graph = HashMap::from([
            (Node::TrackIn, vec![Node::PluginA]),
            (Node::PluginA, vec![Node::TrackOut]),
            (Node::TrackOut, vec![]),
        ]);

        assert!(!would_create_cycle(
            &Node::TrackIn,
            &Node::TrackOut,
            |node| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn allows_edge_between_disconnected_subgraphs() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec![]),
            ("X".to_string(), vec!["Y".to_string()]),
            ("Y".to_string(), vec![]),
        ]);

        assert!(!would_create_cycle(
            &"A".to_string(),
            &"X".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn self_edge_is_always_a_cycle() {
        let graph = HashMap::<String, Vec<String>>::new();
        let node = "A".to_string();
        assert!(would_create_cycle(&node, &node, |current: &String| {
            graph.get(current).cloned().unwrap_or_default()
        }));
    }

    #[test]
    fn detects_long_cycle() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec!["C".to_string()]),
            ("C".to_string(), vec!["D".to_string()]),
            ("D".to_string(), vec!["E".to_string()]),
            ("E".to_string(), vec!["A".to_string()]), // Creates cycle
        ]);

        // Connecting any node in the cycle to any other should detect the cycle
        assert!(would_create_cycle(
            &"E".to_string(),
            &"A".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn allows_connecting_sibling_nodes() {
        // Two disconnected trees
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec![]),
            ("C".to_string(), vec!["D".to_string()]),
            ("D".to_string(), vec![]),
        ]);

        // Adding edge from one tree to another should not create cycle
        assert!(!would_create_cycle(
            &"B".to_string(),
            &"C".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn diamond_graph_connecting_new_node() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string(), "C".to_string()]),
            ("B".to_string(), vec!["D".to_string()]),
            ("C".to_string(), vec!["D".to_string()]),
            ("D".to_string(), vec![]),
        ]);

        // Connecting a new node E to D should not create a cycle
        assert!(!would_create_cycle(
            &"E".to_string(),
            &"D".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn empty_graph_allows_any_edge() {
        let graph: HashMap<String, Vec<String>> = HashMap::new();

        assert!(!would_create_cycle(
            &"A".to_string(),
            &"B".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn detects_cycle_in_bidirectional_graph() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec!["A".to_string()]),
        ]);

        // A->B->A already forms a cycle
        assert!(would_create_cycle(
            &"B".to_string(),
            &"A".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn handles_missing_nodes() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string()]),
            ("B".to_string(), vec!["C".to_string()]),
        ]);

        // Connecting to/from a node not in the graph
        assert!(!would_create_cycle(
            &"X".to_string(),
            &"A".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn single_node_no_edges() {
        let graph = HashMap::from([("A".to_string(), vec![])]);

        // Self-edge on single node is a cycle
        assert!(would_create_cycle(
            &"A".to_string(),
            &"A".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }

    #[test]
    fn connecting_to_new_node_from_diamond() {
        let graph = HashMap::from([
            ("A".to_string(), vec!["B".to_string(), "C".to_string()]),
            ("B".to_string(), vec!["D".to_string()]),
            ("C".to_string(), vec!["D".to_string()]),
            ("D".to_string(), vec!["E".to_string()]),
            ("E".to_string(), vec![]),
        ]);

        // Connecting a new node F to E should be allowed
        assert!(!would_create_cycle(
            &"F".to_string(),
            &"E".to_string(),
            |node: &String| { graph.get(node).cloned().unwrap_or_default() }
        ));
    }
}
