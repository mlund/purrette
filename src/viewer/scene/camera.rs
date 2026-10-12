//! Orbiting perspective camera. A plain value, so picking can be tested without a window.

use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    target: Vec3,
    rotation: Quat,
    distance: f32,
    /// Vertical field of view, radians.
    fov_y: f32,
    /// Viewport size in points.
    viewport: [f32; 2],
}

impl Camera {
    pub fn new(viewport: [f32; 2]) -> Self {
        Self {
            target: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            distance: 50.0,
            fov_y: 35f32.to_radians(),
            viewport,
        }
    }

    pub fn set_viewport(&mut self, viewport: [f32; 2]) {
        self.viewport = viewport;
    }

    /// Frame a bounding sphere with a small margin.
    pub fn fit(&mut self, center: [f32; 3], radius: f32) {
        self.target = Vec3::from(center);
        self.distance = 1.15 * radius.max(1.0) / (0.5 * self.fov_y.min(self.fov_x())).sin();
    }

    /// Trackball-style rotation from a pointer drag in points.
    pub fn rotate(&mut self, delta: [f32; 2]) {
        const RADIANS_PER_POINT: f32 = 0.01;
        let yaw = Quat::from_axis_angle(Vec3::Y, -delta[0] * RADIANS_PER_POINT);
        let pitch = Quat::from_axis_angle(Vec3::X, -delta[1] * RADIANS_PER_POINT);
        // Applied in view space so drags always turn the molecule the way the pointer moves.
        self.rotation = (self.rotation * yaw * pitch).normalize();
    }

    /// Move the target in the view plane by a drag in points.
    pub fn pan(&mut self, delta: [f32; 2]) {
        let per_point = 2.0 * self.distance * (0.5 * self.fov_y).tan() / self.viewport[1];
        let shift = self.rotation * Vec3::new(-delta[0], delta[1], 0.0) * per_point;
        self.target += shift;
    }

    /// Zoom by a factor (> 1 moves closer).
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance / factor.max(1e-3)).clamp(1.0, 1e5);
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.rotation * Vec3::new(0.0, 0.0, self.distance)
    }

    /// Screen right and up directions in world space, for camera-facing billboards.
    pub fn basis(&self) -> (Vec3, Vec3) {
        (self.rotation * Vec3::X, self.rotation * Vec3::Y)
    }

    pub fn view_proj(&self) -> Mat4 {
        let up = self.rotation * Vec3::Y;
        let view = Mat4::look_at_rh(self.eye(), self.target, up);
        let aspect = self.viewport[0] / self.viewport[1].max(1.0);
        let near = (self.distance * 0.01).max(0.1);
        let proj = Mat4::perspective_rh(self.fov_y, aspect, near, self.distance * 10.0);
        proj * view
    }

    /// Point in Å to viewport position in points; None if behind the camera.
    #[cfg(test)]
    pub fn project(&self, p: [f32; 3]) -> Option<[f32; 2]> {
        use glam::Vec4Swizzles as _;
        let clip = self.view_proj() * Vec3::from(p).extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some([
            (ndc.x + 1.0) * 0.5 * self.viewport[0],
            (1.0 - ndc.y) * 0.5 * self.viewport[1],
        ])
    }

    /// Ray (origin, unit direction) through a viewport position in points.
    pub fn ray(&self, pixel: [f32; 2]) -> (Vec3, Vec3) {
        let x = 2.0 * pixel[0] / self.viewport[0] - 1.0;
        let y = 1.0 - 2.0 * pixel[1] / self.viewport[1];
        let inv = self.view_proj().inverse();
        let near = inv.project_point3(Vec3::new(x, y, 0.0));
        let far = inv.project_point3(Vec3::new(x, y, 1.0));
        (near, (far - near).normalize())
    }

    fn fov_x(&self) -> f32 {
        let aspect = self.viewport[0] / self.viewport[1].max(1.0);
        2.0 * ((0.5 * self.fov_y).tan() * aspect).atan()
    }
}
