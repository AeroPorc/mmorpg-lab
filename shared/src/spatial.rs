use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }
}


pub struct QuadTree {
    bounds: Rect,
    #[allow(dead_code)]
    depth: u8,
    #[allow(dead_code)]
    max_depth: u8,
    children: Option<Box<[QuadTree; 4]>>,
    shard_id: Option<u32>,
}

impl QuadTree {
  
    pub fn build(world: Rect, max_depth: u8, shard_width: f32) -> Self {
        Self::build_node(world, 0, max_depth, shard_width)
    }

    fn build_node(bounds: Rect, depth: u8, max_depth: u8, shard_width: f32) -> Self {
        if depth >= max_depth {
            let band = (bounds.center().x / shard_width).floor().max(0.0) as u32;
            return Self {
                bounds,
                depth,
                max_depth,
                children: None,
                shard_id: Some(band),
            };
        }

        let c = bounds.center();
        let make = |min: Vec2, max: Vec2| {
            Box::new(Self::build_node(Rect::new(min, max), depth + 1, max_depth, shard_width))
        };

        let children = [
            *make(bounds.min, c),
            *make(Vec2::new(c.x, bounds.min.y), Vec2::new(bounds.max.x, c.y)),
            *make(Vec2::new(bounds.min.x, c.y), Vec2::new(c.x, bounds.max.y)),
            *make(c, bounds.max),
        ];

        Self {
            bounds,
            depth,
            max_depth,
            children: Some(Box::new(children)),
            shard_id: None,
        }
    }

    pub fn shard_for(&self, pos: Vec2) -> Option<u32> {
        if !self.bounds.contains(pos) {
            return None;
        }
        match &self.children {
            None => self.shard_id,
            Some(children) => children.iter().find_map(|child| child.shard_for(pos)),
        }
    }

    pub fn shards_near(&self, pos: Vec2, margin: f32) -> Vec<u32> {
        let query = Rect::new(pos - Vec2::splat(margin), pos + Vec2::splat(margin));
        let mut found = Vec::new();
        self.collect_intersecting(&query, &mut found);
        found.sort_unstable();
        found.dedup();
        found
    }

    fn collect_intersecting(&self, query: &Rect, out: &mut Vec<u32>) {
        if !self.bounds.intersects(query) {
            return;
        }
        match &self.children {
            None => {
                if let Some(shard) = self.shard_id {
                    out.push(shard);
                }
            }
            Some(children) => {
                for child in children.iter() {
                    child.collect_intersecting(query, out);
                }
            }
        }
    }
    pub fn leaf_for(&self, pos: Vec2) -> Option<Rect> {
        if !self.bounds.contains(pos) {
            return None;
        }
        match &self.children {
            None => Some(self.bounds),
            Some(children) => children.iter().find_map(|child| child.leaf_for(pos)),
        }
    }
    pub fn same_leaf(&self, a: Vec2, b: Vec2) -> bool {
        match (self.leaf_for(a), self.leaf_for(b)) {
            (Some(la), Some(lb)) => la == lb,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARD_WIDTH: f32 = 256.0;

    fn world(shard_count: u32) -> QuadTree {
        let bounds = Rect::new(
            Vec2::ZERO,
            Vec2::new(SHARD_WIDTH * shard_count as f32, SHARD_WIDTH),
        );
        QuadTree::build(bounds, 4, SHARD_WIDTH)
    }

    #[test]
    fn shard_for_maps_x_bands() {
        let tree = world(4);
        assert_eq!(tree.shard_for(Vec2::new(10.0, 10.0)), Some(0));
        assert_eq!(tree.shard_for(Vec2::new(300.0, 200.0)), Some(1));
        assert_eq!(tree.shard_for(Vec2::new(520.0, 5.0)), Some(2));
        assert_eq!(tree.shard_for(Vec2::new(1000.0, 250.0)), Some(3));
    }

    #[test]
    fn shard_for_outside_world_is_none() {
        let tree = world(4);
        assert_eq!(tree.shard_for(Vec2::new(-1.0, 10.0)), None);
        assert_eq!(tree.shard_for(Vec2::new(10.0, 999.0)), None);
    }

    #[test]
    fn shards_near_detects_a_single_shard_in_the_interior() {
        let tree = world(4);
        assert_eq!(tree.shards_near(Vec2::new(380.0, 128.0), 8.0), vec![1]);
    }

    #[test]
    fn shards_near_detects_a_frontier() {
        let tree = world(4);
        let near = tree.shards_near(Vec2::new(256.0, 128.0), 24.0);
        assert!(near.contains(&0), "expected shard 0 in {:?}", near);
        assert!(near.contains(&1), "expected shard 1 in {:?}", near);
        assert_eq!(near.len(), 2);
    }

    #[test]
    fn same_leaf_is_true_within_a_cell_and_false_across_cells() {
        let tree = world(4);
        let a = Vec2::new(10.0, 4.0);
        let b = Vec2::new(20.0, 8.0); 
        let c = Vec2::new(200.0, 4.0);
        assert!(tree.same_leaf(a, b));
        assert!(!tree.same_leaf(a, c));
    }

    #[test]
    fn leaf_for_returns_the_containing_cell() {
        let tree = world(4);
        let leaf = tree.leaf_for(Vec2::new(10.0, 4.0)).expect("inside world");
        assert!(leaf.contains(Vec2::new(10.0, 4.0)));

        assert!((leaf.max.x - leaf.min.x - 64.0).abs() < 0.001);
        assert!((leaf.max.y - leaf.min.y - 16.0).abs() < 0.001);
    }
}
