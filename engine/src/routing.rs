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
}
