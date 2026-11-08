//! The groups `in the <region> group` and `in my group` name (docs/COMMANDS.md section 6): a query's
//! candidates clustered by single linkage (two share a group when a chain of them, each within the
//! link distance of the next, joins them), each group named by the compass region of its centre
//! from the map's centre.

use super::ast::Region;
use crate::math::Vec3;

/// The groups of `points`: each a list of indices, in index order, groups by their first index.
pub fn clusters(points: &[Vec3], link: f32) -> Vec<Vec<usize>> {
    // Union-find over every pair within the link distance (a match's candidates are a few hundred).
    let mut parent: Vec<usize> = (0..points.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            if (points[i] - points[j]).length_xy() <= link {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut index_of_root: Vec<Option<usize>> = vec![None; points.len()];
    for i in 0..points.len() {
        let r = root(&mut parent, i);
        match index_of_root[r] {
            Some(g) => groups[g].push(i),
            None => {
                index_of_root[r] = Some(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups
}

/// The direction a region lies in from the map's centre (radians, east 0, north π/2); `None` for
/// the centre.
fn bearing(r: Region) -> Option<f32> {
    use std::f32::consts::FRAC_PI_4 as Q;
    Some(match r {
        Region::Eastern => 0.0,
        Region::Northeastern => Q,
        Region::Northern => 2.0 * Q,
        Region::Northwestern => 3.0 * Q,
        Region::Western => 4.0 * Q,
        Region::Southwestern => -3.0 * Q,
        Region::Southern => -2.0 * Q,
        Region::Southeastern => -Q,
        Region::Central => return None,
    })
}

fn angle_between(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    d.min(std::f32::consts::TAU - d)
}

/// Each group's region: its centre's compass region from the map's centre (north is +y, the
/// radar's top), or central within `central` of it. Two groups never share a name: the one farther
/// out keeps it and the other takes the free region nearest its bearing (central last).
pub fn names(groups: &[Vec<usize>], points: &[Vec3], centre: Vec3, central: f32) -> Vec<Region> {
    let centroids: Vec<Vec3> = groups
        .iter()
        .map(|g| {
            let sum = g.iter().fold(Vec3::ZERO, |s, &i| s + points[i]);
            sum * (1.0 / g.len().max(1) as f32)
        })
        .collect();
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| (centroids[b] - centre).length_xy().total_cmp(&(centroids[a] - centre).length_xy()).then(a.cmp(&b)));
    let mut out = vec![Region::Central; groups.len()];
    let mut taken: Vec<Region> = Vec::new();
    for g in order {
        let d = centroids[g] - centre;
        let at = d.y.atan2(d.x);
        let mut prefs: Vec<Region> = Region::ALL.to_vec();
        // Nearest bearing first; the centre first of all when the group stands in it.
        prefs.sort_by(|&a, &b| {
            let key = |r: Region| match bearing(r) {
                Some(br) => angle_between(at, br),
                None if d.length_xy() <= central => -1.0,
                None => f32::INFINITY,
            };
            key(a).total_cmp(&key(b))
        });
        if let Some(&r) = prefs.iter().find(|r| !taken.contains(r)) {
            out[g] = r;
            taken.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    #[test]
    fn single_linkage_joins_chains_within_the_link() {
        // A chain 0-1-2 (each 90 apart), a lone 3, a pair 4-5.
        let pts = [p(0.0, 0.0), p(90.0, 0.0), p(180.0, 0.0), p(1000.0, 0.0), p(0.0, 900.0), p(50.0, 900.0)];
        assert_eq!(clusters(&pts, 100.0), vec![vec![0, 1, 2], vec![3], vec![4, 5]]);
        assert_eq!(clusters(&pts, 80.0).len(), 5);
        assert!(clusters(&[], 100.0).is_empty());
    }

    #[test]
    fn groups_are_named_by_their_bearing_and_never_share_a_name() {
        let centre = p(1000.0, 1000.0);
        let pts = [p(1000.0, 1900.0), p(1000.0, 100.0), p(1900.0, 1000.0), p(1010.0, 990.0)];
        let groups: Vec<Vec<usize>> = (0..4).map(|i| vec![i]).collect();
        assert_eq!(names(&groups, &pts, centre, 200.0), [Region::Northern, Region::Southern, Region::Eastern, Region::Central]);
        // Two to the north: the farther keeps it, the nearer takes the nearest free bearing.
        let pts = [p(1000.0, 1900.0), p(1050.0, 1500.0)];
        let groups = vec![vec![0], vec![1]];
        let n = names(&groups, &pts, centre, 200.0);
        assert_eq!(n[0], Region::Northern);
        assert_eq!(n[1], Region::Northeastern);
    }
}
