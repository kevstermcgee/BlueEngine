//! A small walkable graph (doorway thresholds, room centres, patrol stops) with shortest-path lookup,
//! for an NPC or monster that needs to move through a custom-sim game's hand-built indoor level — a
//! different need from [`super::path::ClosedPath`]'s continuous closed loop (a track, a patrol lap).
//! Pairs with [`super::level`]'s wall builders, which make the rooms this graph connects.
use crate::math::V;

/// One node: a position and the indices of the nodes it connects to.
#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    pub pos: V,
    pub edges: Vec<usize>,
}

/// A graph of [`Waypoint`]s with breadth-first shortest-path and nearest-node lookup. Builds once (from
/// authored data or code); an empty graph is valid and simply has no reachable nodes.
#[derive(Clone, Debug, Default)]
pub struct WaypointGraph {
    nodes: Vec<Waypoint>,
}

impl WaypointGraph {
    pub fn new(nodes: Vec<Waypoint>) -> Self {
        Self { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The position of node `index`, or `V::ZERO` if out of range.
    pub fn position(&self, index: usize) -> V {
        self.nodes.get(index).map_or(V::ZERO, |n| n.pos)
    }

    /// The node whose position is closest to `pos` by straight-line distance, or `0` for an empty graph.
    pub fn nearest(&self, pos: V) -> usize {
        self.nodes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.pos - pos)
                    .length()
                    .partial_cmp(&(b.pos - pos).length())
                    .unwrap()
            })
            .map_or(0, |(i, _)| i)
    }

    /// Breadth-first shortest path (by hop count) from `from` to `to`, as node indices including both
    /// ends. Empty if either index is out of range or `to` is unreachable from `from`.
    pub fn path(&self, from: usize, to: usize) -> Vec<usize> {
        if from >= self.nodes.len() || to >= self.nodes.len() {
            return Vec::new();
        }
        if from == to {
            return vec![from];
        }
        let mut prev = vec![usize::MAX; self.nodes.len()];
        let mut visited = vec![false; self.nodes.len()];
        let mut queue = std::collections::VecDeque::new();
        visited[from] = true;
        queue.push_back(from);
        while let Some(at) = queue.pop_front() {
            if at == to {
                break;
            }
            for &next in &self.nodes[at].edges {
                if next < self.nodes.len() && !visited[next] {
                    visited[next] = true;
                    prev[next] = at;
                    queue.push_back(next);
                }
            }
        }
        if !visited[to] {
            return Vec::new();
        }
        let mut route = vec![to];
        let mut at = to;
        while at != from {
            at = prev[at];
            route.push(at);
        }
        route.reverse();
        route
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small tree: 0 - 1 - 2, 1 - 3, matching the shape a corridor with two side rooms would have.
    fn tree() -> WaypointGraph {
        WaypointGraph::new(vec![
            Waypoint {
                pos: V(0., 0., 0.),
                edges: vec![1],
            },
            Waypoint {
                pos: V(1., 0., 0.),
                edges: vec![0, 2, 3],
            },
            Waypoint {
                pos: V(2., 0., 0.),
                edges: vec![1],
            },
            Waypoint {
                pos: V(1., 0., 1.),
                edges: vec![1],
            },
        ])
    }

    #[test]
    fn path_reaches_every_node_from_every_other_node() {
        let g = tree();
        for from in 0..g.len() {
            for to in 0..g.len() {
                let route = g.path(from, to);
                assert!(!route.is_empty(), "{from} -> {to}");
                assert_eq!(route[0], from);
                assert_eq!(*route.last().unwrap(), to);
            }
        }
    }

    #[test]
    fn path_is_out_of_range_safe_and_empty_for_unreachable_nodes() {
        let g = tree();
        assert_eq!(g.path(0, 99), Vec::<usize>::new());
        assert_eq!(g.path(99, 0), Vec::<usize>::new());
        let disconnected = WaypointGraph::new(vec![
            Waypoint {
                pos: V::ZERO,
                edges: vec![],
            },
            Waypoint {
                pos: V(5., 0., 0.),
                edges: vec![],
            },
        ]);
        assert_eq!(disconnected.path(0, 1), Vec::<usize>::new());
    }

    #[test]
    fn nearest_finds_the_closest_node_and_handles_an_empty_graph() {
        let g = tree();
        assert_eq!(g.nearest(V(1.9, 0., 0.1)), 2);
        assert_eq!(g.nearest(V(1., 0., 0.9)), 3);
        assert_eq!(WaypointGraph::default().nearest(V::ZERO), 0);
    }

    #[test]
    fn position_is_out_of_range_safe() {
        let g = tree();
        assert_eq!(g.position(0), V(0., 0., 0.));
        assert_eq!(g.position(99), V::ZERO);
    }
}
