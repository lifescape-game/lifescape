use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{
    prelude::*,
    render::mesh::{Indices, VertexAttributeValues},
};
use bevy_xpbd_3d::prelude::*;
use itertools::{Itertools, MinMaxResult};

use super::{
    triangulator::Triangulator, Aperture, Apertures, PointKind, Wall, WallConnection,
    WallConnections,
};
use crate::core::math::segment::Segment;

const WIDTH: f32 = 0.15;
const HEIGHT: f32 = 2.8;
pub(crate) const HALF_WIDTH: f32 = WIDTH / 2.0;

#[derive(Default)]
pub(super) struct WallMesh {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl WallMesh {
    pub(super) fn take(mesh: &mut Mesh) -> Self {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("all wall meshes should have positions");
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) =
            mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("all wall meshes should have UVs");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("all wall meshes should have normals");
        };
        let Some(Indices::U32(indices)) = mesh.remove_indices() else {
            panic!("all wall meshes should have U32 indices");
        };

        Self {
            positions,
            uvs,
            normals,
            indices,
        }
    }

    pub(super) fn generate(
        &mut self,
        wall: Wall,
        connections: &WallConnections,
        apertures: &Apertures,
        triangulator: &mut Triangulator,
    ) {
        self.clear();

        if wall.start == wall.end {
            return;
        }

        let disp = wall.displacement();
        let angle = -disp.to_angle();
        let width = wall_width(disp);
        let rotation_mat = Mat2::from_angle(angle);

        let start_connections = minmax_angles(disp, PointKind::Start, &connections.start);
        let (start_left, start_right) = offset_points(*wall, start_connections, width);

        let end_connections = minmax_angles(-disp, PointKind::End, &connections.end);
        let (end_right, end_left) = offset_points(wall.inverse(), end_connections, -width);

        self.generate_top(
            *wall,
            start_left,
            start_right,
            end_left,
            end_right,
            rotation_mat,
        );

        let inverse_winding = angle.abs() < FRAC_PI_2;
        let quat = Quat::from_axis_angle(Vec3::Y, angle);

        triangulator.set_inverse_winding(inverse_winding);
        self.generate_side(
            *wall,
            apertures,
            triangulator,
            start_right,
            end_right,
            -width,
            rotation_mat,
            quat,
        );

        triangulator.set_inverse_winding(!inverse_winding);
        self.generate_side(
            *wall,
            apertures,
            triangulator,
            start_left,
            end_left,
            width,
            rotation_mat,
            quat,
        );

        match start_connections {
            MinMaxResult::OneElement(_) => (),
            MinMaxResult::NoElements => self.generate_front(start_left, start_right, disp),
            MinMaxResult::MinMax(_, _) => self.generate_start_connection(*wall),
        }

        match end_connections {
            MinMaxResult::OneElement(_) => (),
            MinMaxResult::NoElements => self.generate_back(end_left, end_right, disp),
            MinMaxResult::MinMax(_, _) => self.generate_end_connection(*wall, rotation_mat),
        }
    }

    fn generate_top(
        &mut self,
        segment: Segment,
        start_left: Vec2,
        start_right: Vec2,
        end_left: Vec2,
        end_right: Vec2,
        rotation_mat: Mat2,
    ) {
        self.positions.push([start_left.x, HEIGHT, start_left.y]);
        self.positions.push([start_right.x, HEIGHT, start_right.y]);
        self.positions.push([end_right.x, HEIGHT, end_right.y]);
        self.positions.push([end_left.x, HEIGHT, end_left.y]);

        self.uvs
            .push((rotation_mat * (start_left - segment.start)).into());
        self.uvs
            .push((rotation_mat * (start_right - segment.start)).into());
        self.uvs
            .push((rotation_mat * (end_right - segment.start)).into());
        self.uvs
            .push((rotation_mat * (end_left - segment.start)).into());

        self.normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);

        self.indices.push(0);
        self.indices.push(3);
        self.indices.push(1);
        self.indices.push(1);
        self.indices.push(3);
        self.indices.push(2);
    }

    fn generate_side(
        &mut self,
        segment: Segment,
        apertures: &Apertures,
        triangulator: &mut Triangulator,
        start_side: Vec2,
        end_side: Vec2,
        width: Vec2,
        rotation_mat: Mat2,
        quat: Quat,
    ) {
        let vertices_start = self.vertices_count();

        self.positions.push([start_side.x, 0.0, start_side.y]);
        let start_uv = rotation_mat * (start_side - segment.start);
        self.uvs.push(start_uv.into());
        let normal = [width.x, 0.0, width.y];
        self.normals.push(normal);

        for aperture in apertures.iter().filter(|aperture| !aperture.hole) {
            self.generate_apertures(segment, aperture, normal, width, rotation_mat, quat);
        }

        self.positions.push([end_side.x, 0.0, end_side.y]);
        self.positions.push([end_side.x, HEIGHT, end_side.y]);
        self.positions.push([start_side.x, HEIGHT, start_side.y]);

        let end_uv = rotation_mat * (end_side - segment.start);
        self.uvs.push(end_uv.into());
        self.uvs.push([end_uv.x, end_uv.y + HEIGHT]);
        self.uvs.push([start_uv.x, start_uv.y + HEIGHT]);

        self.normals.extend_from_slice(&[normal; 3]);

        let mut last_index = self.vertices_count() - vertices_start;
        for aperture in apertures.iter().filter(|aperture| aperture.hole) {
            self.generate_apertures(segment, aperture, normal, width, rotation_mat, quat);

            triangulator.add_hole(last_index);
            last_index += aperture.cutout.len() as u32;
        }

        for &index in triangulator.triangulate(&self.positions[vertices_start as usize..]) {
            self.indices.push(vertices_start + index);
        }
    }

    fn generate_apertures(
        &mut self,
        segment: Segment,
        aperture: &Aperture,
        normal: [f32; 3],
        width: Vec2,
        rotation_mat: Mat2,
        quat: Quat,
    ) {
        for &position in &aperture.cutout {
            let translated = quat * position.extend(0.0)
                + aperture.translation
                + Vec3::new(width.x, 0.0, width.y);

            self.positions.push(translated.into());

            let bottom_uv = rotation_mat * (translated.xz() - segment.start);
            self.uvs.push([bottom_uv.x, bottom_uv.y + position.y]);

            self.normals.push(normal)
        }
    }

    fn generate_front(&mut self, start_left: Vec2, start_right: Vec2, disp: Vec2) {
        let vertices_start = self.vertices_count();

        self.positions.push([start_left.x, 0.0, start_left.y]);
        self.positions.push([start_left.x, HEIGHT, start_left.y]);
        self.positions.push([start_right.x, HEIGHT, start_right.y]);
        self.positions.push([start_right.x, 0.0, start_right.y]);

        self.uvs.push([0.0, 0.0]);
        self.uvs.push([0.0, HEIGHT]);
        self.uvs.push([WIDTH, HEIGHT]);
        self.uvs.push([WIDTH, 0.0]);

        self.normals
            .extend_from_slice(&[[-disp.x, 0.0, -disp.y]; 4]);

        self.indices.push(vertices_start);
        self.indices.push(vertices_start + 1);
        self.indices.push(vertices_start + 3);
        self.indices.push(vertices_start + 1);
        self.indices.push(vertices_start + 2);
        self.indices.push(vertices_start + 3);
    }

    fn generate_back(&mut self, end_left: Vec2, end_right: Vec2, disp: Vec2) {
        let vertices_start = self.vertices_count();

        // Back
        self.positions.push([end_left.x, 0.0, end_left.y]);
        self.positions.push([end_left.x, HEIGHT, end_left.y]);
        self.positions.push([end_right.x, HEIGHT, end_right.y]);
        self.positions.push([end_right.x, 0.0, end_right.y]);

        self.uvs.push([0.0, 0.0]);
        self.uvs.push([0.0, HEIGHT]);
        self.uvs.push([WIDTH, HEIGHT]);
        self.uvs.push([WIDTH, 0.0]);

        self.normals.extend_from_slice(&[[disp.x, 0.0, disp.y]; 4]);

        self.indices.push(vertices_start);
        self.indices.push(vertices_start + 3);
        self.indices.push(vertices_start + 1);
        self.indices.push(vertices_start + 1);
        self.indices.push(vertices_start + 3);
        self.indices.push(vertices_start + 2);
    }

    /// Inside triangle to fill the gap between 3+ walls.
    fn generate_start_connection(&mut self, segment: Segment) {
        let vertices_start = self.vertices_count();

        // Inside triangle to fill the gap between 3+ walls.
        self.positions
            .push([segment.start.x, HEIGHT, segment.start.y]);
        self.uvs.push([0.0, 0.0]);
        self.normals.push([0.0, 1.0, 0.0]);

        self.indices.push(1);
        self.indices.push(vertices_start);
        self.indices.push(0);
    }

    /// Inside triangle to fill the gap between 3+ walls.
    fn generate_end_connection(&mut self, segment: Segment, rotation_mat: Mat2) {
        let vertices_start = self.vertices_count();

        self.positions.push([segment.end.x, HEIGHT, segment.end.y]);
        self.uvs
            .push((rotation_mat * (segment.end - segment.start)).into());
        self.normals.push([0.0, 1.0, 0.0]);

        self.indices.push(3);
        self.indices.push(vertices_start);
        self.indices.push(2);
    }

    fn vertices_count(&self) -> u32 {
        self.positions
            .len()
            .try_into()
            .expect("vertices should fit u32")
    }

    fn clear(&mut self) {
        self.positions.clear();
        self.uvs.clear();
        self.normals.clear();
        self.indices.clear();
    }

    pub(super) fn apply(self, mesh: &mut Mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_indices(Indices::U32(self.indices))
    }
}

/// Calculates the wall thickness vector that faces to the left relative to the wall vector.
fn wall_width(disp: Vec2) -> Vec2 {
    disp.perp().normalize() * HALF_WIDTH
}

/// Calculates the left and right wall points for the `start` point of the wall segment,
/// considering intersections with other wall segments.
fn offset_points(
    segment: Segment,
    connections: MinMaxResult<Segment>,
    width: Vec2,
) -> (Vec2, Vec2) {
    match connections {
        MinMaxResult::NoElements => (segment.start + width, segment.start - width),
        MinMaxResult::OneElement(other_segment) => {
            let other_width = wall_width(other_segment.displacement());
            let left = (segment + width)
                .line_intersection(other_segment - other_width)
                .unwrap_or_else(|| segment.start + width);
            let right = (segment - width)
                .line_intersection(other_segment.inverse() + other_width)
                .unwrap_or_else(|| segment.start + width);

            (left, right)
        }
        MinMaxResult::MinMax(min_segment, max_segment) => {
            let max_width = wall_width(max_segment.displacement());
            let left = (segment + width)
                .line_intersection(max_segment - max_width)
                .unwrap_or_else(|| segment.start + width);
            let min_width = wall_width(min_segment.displacement());
            let right = (segment - width)
                .line_intersection(min_segment.inverse() + min_width)
                .unwrap_or_else(|| segment.start + width);

            (left, right)
        }
    }
}

/// Returns the segments with the maximum and minimum angle relative
/// to the displacement vector.
fn minmax_angles(
    disp: Vec2,
    point_kind: PointKind,
    point_connections: &[WallConnection],
) -> MinMaxResult<Segment> {
    point_connections
        .iter()
        .map(|connection| {
            // Rotate points based on connection type.
            match (point_kind, connection.point_kind) {
                (PointKind::Start, PointKind::End) => connection.segment.inverse(),
                (PointKind::End, PointKind::Start) => connection.segment,
                (PointKind::Start, PointKind::Start) => connection.segment,
                (PointKind::End, PointKind::End) => connection.segment.inverse(),
            }
        })
        .minmax_by_key(|segment| {
            let angle = segment.displacement().angle_between(disp);
            if angle < 0.0 {
                angle + 2.0 * PI
            } else {
                angle
            }
        })
}

/// Generates a simplified collider consists of cuboids.
///
/// Clippings split the collider into separate cuboids.
/// We generate a trimesh since navigation doesn't support compound shapes.
pub(super) fn generate_collider(wall: Wall, apertures: &Apertures) -> Collider {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut start = wall.start;
    let wall_dir = wall.displacement().normalize();
    for aperture in apertures
        .iter()
        .filter(|aperture| !aperture.hole && !aperture.placing_object)
    {
        let first = aperture.cutout.first().expect("apertures can't be empty");
        let mut end = aperture.translation.xz();
        end += first.x * wall_dir;

        generate_cuboid(&mut vertices, &mut indices, start, end);

        let last = aperture.cutout.last().unwrap();
        start = aperture.translation.xz();
        start += last.x * wall_dir;
    }

    generate_cuboid(&mut vertices, &mut indices, start, wall.end);

    Collider::trimesh(vertices, indices)
}

fn generate_cuboid(vertices: &mut Vec<Vec3>, indices: &mut Vec<[u32; 3]>, start: Vec2, end: Vec2) {
    let last_index = vertices.len().try_into().expect("vertices should fit u32");

    let width_disp = wall_width(end - start);
    let left_start = start + width_disp;
    let right_start = start - width_disp;
    let left_end = end + width_disp;
    let right_end = end - width_disp;

    vertices.push(Vec3::new(left_start.x, 0.0, left_start.y));
    vertices.push(Vec3::new(right_start.x, 0.0, right_start.y));
    vertices.push(Vec3::new(right_end.x, 0.0, right_end.y));
    vertices.push(Vec3::new(left_end.x, 0.0, left_end.y));

    vertices.push(Vec3::new(left_start.x, HEIGHT, left_start.y));
    vertices.push(Vec3::new(right_start.x, HEIGHT, right_start.y));
    vertices.push(Vec3::new(right_end.x, HEIGHT, right_end.y));
    vertices.push(Vec3::new(left_end.x, HEIGHT, left_end.y));

    // Top
    indices.push([last_index + 5, last_index + 4, last_index + 6]);
    indices.push([last_index + 4, last_index + 7, last_index + 6]);

    // Left
    indices.push([last_index + 3, last_index + 4, last_index]);
    indices.push([last_index + 3, last_index + 7, last_index + 4]);

    // Right
    indices.push([last_index + 1, last_index + 5, last_index + 2]);
    indices.push([last_index + 5, last_index + 6, last_index + 2]);

    // Back
    indices.push([last_index, last_index + 5, last_index + 1]);
    indices.push([last_index, last_index + 4, last_index + 5]);

    // Front
    indices.push([last_index + 2, last_index + 6, last_index + 3]);
    indices.push([last_index + 6, last_index + 7, last_index + 3]);
}
