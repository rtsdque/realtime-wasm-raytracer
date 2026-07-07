use wasm_bindgen::prelude::*;

const EPS: f32 = 0.001;
const INF: f32 = 1.0e30;

#[wasm_bindgen]
pub struct Renderer {
    width: u32,
    height: u32,
    frame: u32,
    scene: Scene,
}

#[wasm_bindgen]
impl Renderer {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Renderer {
        Renderer {
            width,
            height,
            frame: 0,
            scene: Scene::new(true),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
    }

    pub fn render(
        &mut self,
        time: f32,
        yaw: f32,
        pitch: f32,
        distance: f32,
        samples: u32,
        max_depth: u32,
        extra_spheres: bool,
    ) -> Vec<u8> {
        self.scene.set_extra_spheres(extra_spheres);
        let samples = samples.clamp(1, 16);
        let max_depth = max_depth.clamp(1, 12);
        let camera = Camera::orbit(
            Vec3::new(0.0, 0.6, 0.0),
            yaw,
            pitch.clamp(-1.25, 1.25),
            distance.clamp(2.2, 9.0),
            self.width as f32 / self.height as f32,
        );
        let light_pos = Vec3::new(time.cos() * 3.4, 4.0, time.sin() * 3.4);
        let mut pixels = vec![0_u8; (self.width * self.height * 4) as usize];

        for y in 0..self.height {
            for x in 0..self.width {
                let mut color = Vec3::ZERO;
                for s in 0..samples {
                    let seed = hash_seed(x, y, s, self.frame);
                    let jx = rand01(seed) - 0.5;
                    let jy = rand01(seed ^ 0x9e37_79b9) - 0.5;
                    let u = (x as f32 + 0.5 + jx) / self.width as f32;
                    let v = (y as f32 + 0.5 + jy) / self.height as f32;
                    let ray = camera.ray(u, v);
                    color += trace_ray(&ray, &self.scene, light_pos, max_depth, seed);
                }
                color /= samples as f32;
                color = Vec3::new(color.x.sqrt(), color.y.sqrt(), color.z.sqrt());
                let i = ((y * self.width + x) * 4) as usize;
                pixels[i] = to_byte(color.x);
                pixels[i + 1] = to_byte(color.y);
                pixels[i + 2] = to_byte(color.z);
                pixels[i + 3] = 255;
            }
        }

        self.frame = self.frame.wrapping_add(1);
        pixels
    }
}

#[derive(Clone, Copy)]
struct Ray {
    origin: Vec3,
    dir: Vec3,
}

impl Ray {
    fn at(self, t: f32) -> Vec3 {
        self.origin + self.dir * t
    }
}

#[derive(Clone, Copy)]
struct Camera {
    origin: Vec3,
    lower_left: Vec3,
    horizontal: Vec3,
    vertical: Vec3,
}

impl Camera {
    fn orbit(target: Vec3, yaw: f32, pitch: f32, distance: f32, aspect: f32) -> Camera {
        let eye = target
            + Vec3::new(
                distance * yaw.sin() * pitch.cos(),
                distance * pitch.sin(),
                distance * yaw.cos() * pitch.cos(),
            );
        let forward = (target - eye).normalized();
        let right = forward.cross(Vec3::Y).normalized();
        let up = right.cross(forward).normalized();
        let viewport_h = 2.0 * (35.0_f32.to_radians() * 0.5).tan();
        let viewport_w = aspect * viewport_h;
        let horizontal = right * viewport_w;
        let vertical = up * viewport_h;
        Camera {
            origin: eye,
            lower_left: eye + forward - horizontal * 0.5 - vertical * 0.5,
            horizontal,
            vertical,
        }
    }

    fn ray(self, u: f32, v: f32) -> Ray {
        Ray {
            origin: self.origin,
            dir: (self.lower_left + self.horizontal * u + self.vertical * (1.0 - v)
                - self.origin)
                .normalized(),
        }
    }
}

#[derive(Clone, Copy)]
struct Hit {
    t: f32,
    p: Vec3,
    normal: Vec3,
    material: Material,
}

#[derive(Clone, Copy)]
enum Material {
    Diffuse { albedo: Vec3 },
    Metal { albedo: Vec3, fuzz: f32 },
    Dielectric { tint: Vec3, ior: f32 },
}

#[derive(Clone, Copy)]
struct Sphere {
    center: Vec3,
    radius: f32,
    material: Material,
    enabled_when_extra: bool,
}

impl Sphere {
    fn hit(self, ray: &Ray, t_min: f32, t_max: f32) -> Option<Hit> {
        let oc = ray.origin - self.center;
        let a = ray.dir.length_squared();
        let half_b = oc.dot(ray.dir);
        let c = oc.length_squared() - self.radius * self.radius;
        let discriminant = half_b * half_b - a * c;
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let mut t = (-half_b - root) / a;
        if t < t_min || t > t_max {
            t = (-half_b + root) / a;
            if t < t_min || t > t_max {
                return None;
            }
        }
        let p = ray.at(t);
        Some(Hit {
            t,
            p,
            normal: (p - self.center) / self.radius,
            material: self.material,
        })
    }

    fn bounds(self) -> Aabb {
        Aabb {
            min: self.center - Vec3::splat(self.radius),
            max: self.center + Vec3::splat(self.radius),
        }
    }
}

#[derive(Clone, Copy)]
struct Plane {
    point: Vec3,
    normal: Vec3,
    material: Material,
}

impl Plane {
    fn hit(self, ray: &Ray, t_min: f32, t_max: f32) -> Option<Hit> {
        let denom = self.normal.dot(ray.dir);
        if denom.abs() < 1.0e-5 {
            return None;
        }
        let t = (self.point - ray.origin).dot(self.normal) / denom;
        if t < t_min || t > t_max {
            return None;
        }
        Some(Hit {
            t,
            p: ray.at(t),
            normal: if denom < 0.0 { self.normal } else { -self.normal },
            material: self.material,
        })
    }
}

struct Scene {
    spheres: Vec<Sphere>,
    planes: Vec<Plane>,
    active_spheres: Vec<usize>,
    bvh: Vec<BvhNode>,
    bvh_root: Option<usize>,
    extra_spheres: bool,
}

impl Scene {
    fn new(extra_spheres: bool) -> Scene {
        let mut scene = Scene {
            spheres: vec![
                Sphere {
                    center: Vec3::new(-1.2, 0.55, 0.0),
                    radius: 0.55,
                    material: Material::Diffuse {
                        albedo: Vec3::new(0.95, 0.28, 0.18),
                    },
                    enabled_when_extra: false,
                },
                Sphere {
                    center: Vec3::new(0.0, 0.6, -0.15),
                    radius: 0.6,
                    material: Material::Dielectric {
                        tint: Vec3::new(0.88, 0.96, 1.0),
                        ior: 1.5,
                    },
                    enabled_when_extra: false,
                },
                Sphere {
                    center: Vec3::new(1.28, 0.5, 0.18),
                    radius: 0.5,
                    material: Material::Metal {
                        albedo: Vec3::new(0.86, 0.76, 0.55),
                        fuzz: 0.12,
                    },
                    enabled_when_extra: false,
                },
                Sphere {
                    center: Vec3::new(-0.45, 0.32, 1.05),
                    radius: 0.32,
                    material: Material::Metal {
                        albedo: Vec3::new(0.55, 0.72, 0.92),
                        fuzz: 0.36,
                    },
                    enabled_when_extra: true,
                },
                Sphere {
                    center: Vec3::new(0.88, 0.28, 1.05),
                    radius: 0.28,
                    material: Material::Diffuse {
                        albedo: Vec3::new(0.34, 0.88, 0.55),
                    },
                    enabled_when_extra: true,
                },
            ],
            planes: vec![Plane {
                point: Vec3::new(0.0, 0.0, 0.0),
                normal: Vec3::Y,
                material: Material::Diffuse {
                    albedo: Vec3::new(0.62, 0.62, 0.58),
                },
            }],
            active_spheres: Vec::new(),
            bvh: Vec::new(),
            bvh_root: None,
            extra_spheres,
        };
        scene.rebuild_bvh();
        scene
    }

    fn set_extra_spheres(&mut self, enabled: bool) {
        if self.extra_spheres != enabled {
            self.extra_spheres = enabled;
            self.rebuild_bvh();
        }
    }

    fn rebuild_bvh(&mut self) {
        self.active_spheres = self
            .spheres
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (!s.enabled_when_extra || self.extra_spheres).then_some(i))
            .collect();
        self.bvh.clear();
        if self.active_spheres.len() > 3 {
            let mut indices = self.active_spheres.clone();
            self.bvh_root = Some(build_bvh(&self.spheres, &mut indices, &mut self.bvh));
        } else {
            self.bvh_root = None;
        }
    }

    fn hit(&self, ray: &Ray, t_min: f32, t_max: f32) -> Option<Hit> {
        let mut best = None;
        let mut closest = t_max;

        if let Some(root) = self.bvh_root {
            best = self.hit_bvh(root, ray, t_min, closest);
            if let Some(hit) = best {
                closest = hit.t;
            }
        } else {
            for &i in &self.active_spheres {
                if let Some(hit) = self.spheres[i].hit(ray, t_min, closest) {
                    closest = hit.t;
                    best = Some(hit);
                }
            }
        }

        for plane in &self.planes {
            if let Some(hit) = plane.hit(ray, t_min, closest) {
                closest = hit.t;
                best = Some(hit);
            }
        }
        best
    }

    fn hit_bvh(&self, node_index: usize, ray: &Ray, t_min: f32, t_max: f32) -> Option<Hit> {
        let node = self.bvh[node_index];
        if !node.bounds.hit(ray, t_min, t_max) {
            return None;
        }
        match node.kind {
            BvhKind::Leaf {
                sphere_a,
                sphere_b,
                len,
            } => {
                let mut best = None;
                let mut closest = t_max;
                for sphere_index in [sphere_a, sphere_b].into_iter().take(len) {
                    if let Some(hit) = self.spheres[sphere_index].hit(ray, t_min, closest) {
                        closest = hit.t;
                        best = Some(hit);
                    }
                }
                best
            }
            BvhKind::Branch { left, right } => {
                let left_hit = self.hit_bvh(left, ray, t_min, t_max);
                let right_max = left_hit.map_or(t_max, |h| h.t);
                let right_hit = self.hit_bvh(right, ray, t_min, right_max);
                right_hit.or(left_hit)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BvhNode {
    bounds: Aabb,
    kind: BvhKind,
}

#[derive(Clone, Copy)]
enum BvhKind {
    Leaf {
        sphere_a: usize,
        sphere_b: usize,
        len: usize,
    },
    Branch { left: usize, right: usize },
}

fn build_bvh(spheres: &[Sphere], indices: &mut [usize], nodes: &mut Vec<BvhNode>) -> usize {
    let bounds = indices
        .iter()
        .map(|&i| spheres[i].bounds())
        .reduce(Aabb::union)
        .unwrap();
    if indices.len() <= 2 {
        let node_index = nodes.len();
        nodes.push(BvhNode {
            bounds,
            kind: BvhKind::Leaf {
                sphere_a: indices[0],
                sphere_b: *indices.get(1).unwrap_or(&indices[0]),
                len: indices.len(),
            },
        });
        return node_index;
    }

    let extent = bounds.max - bounds.min;
    let axis = if extent.x > extent.y && extent.x > extent.z {
        0
    } else if extent.y > extent.z {
        1
    } else {
        2
    };
    indices.sort_by(|&a, &b| {
        spheres[a]
            .center
            .axis(axis)
            .partial_cmp(&spheres[b].center.axis(axis))
            .unwrap()
    });
    let mid = indices.len() / 2;
    let (left_indices, right_indices) = indices.split_at_mut(mid);
    let left = build_bvh(spheres, left_indices, nodes);
    let right = build_bvh(spheres, right_indices, nodes);
    let node_index = nodes.len();
    nodes.push(BvhNode {
        bounds,
        kind: BvhKind::Branch { left, right },
    });
    node_index
}

fn trace_ray(ray: &Ray, scene: &Scene, light_pos: Vec3, max_depth: u32, seed: u32) -> Vec3 {
    // This iterative loop is the main hot path; it is the right place to split rows
    // across Web Workers or try wasm SIMD once the scalar version needs more headroom.
    let mut ray = *ray;
    let mut throughput = Vec3::splat(1.0);
    let mut radiance = Vec3::ZERO;
    for bounce in 0..max_depth {
        let Some(hit) = scene.hit(&ray, EPS, INF) else {
            let sky_t = 0.5 * (ray.dir.y + 1.0);
            radiance += throughput
                * Vec3::new(0.42, 0.56, 0.78).lerp(Vec3::new(0.03, 0.04, 0.07), sky_t);
            break;
        };

        let ambient = 0.08;
        match hit.material {
            Material::Diffuse { albedo } => {
                radiance += throughput * albedo * (ambient + direct_light(scene, &hit, light_pos));
                let target = hit.p + hit.normal + random_unit(seed, bounce);
                ray = Ray {
                    origin: hit.p + hit.normal * EPS,
                    dir: (target - hit.p).normalized(),
                };
                throughput *= albedo * 0.35;
            }
            Material::Metal { albedo, fuzz } => {
                radiance += throughput * albedo * (ambient + direct_light(scene, &hit, light_pos));
                let reflected = reflect(ray.dir.normalized(), hit.normal);
                ray = Ray {
                    origin: hit.p + hit.normal * EPS,
                    dir: (reflected + random_unit(seed, bounce) * fuzz).normalized(),
                };
                throughput *= albedo * 0.82;
                if ray.dir.dot(hit.normal) <= 0.0 {
                    break;
                }
            }
            Material::Dielectric { tint, ior } => {
                radiance += throughput * tint * ambient * 0.35;
                let front_face = ray.dir.dot(hit.normal) < 0.0;
                let normal = if front_face { hit.normal } else { -hit.normal };
                let eta = if front_face { 1.0 / ior } else { ior };
                let cos_theta = (-ray.dir).dot(normal).min(1.0);
                let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
                let reflect_prob = schlick(cos_theta, eta);
                let cannot_refract = eta * sin_theta > 1.0;
                let reflected = reflect(ray.dir, normal);
                let refracted = refract(ray.dir, normal, eta);
                let choose_reflect = cannot_refract || rand01(seed ^ bounce.wrapping_mul(7919)) < reflect_prob;
                ray = Ray {
                    origin: hit.p + if choose_reflect { normal } else { -normal } * EPS,
                    dir: if choose_reflect { reflected } else { refracted }.normalized(),
                };
                throughput *= tint * 0.96;
            }
        }
    }
    radiance.clamped(0.0, 1.0)
}

fn direct_light(scene: &Scene, hit: &Hit, light_pos: Vec3) -> f32 {
    let to_light = light_pos - hit.p;
    let dist = to_light.length();
    let light_dir = to_light / dist;
    let ndotl = hit.normal.dot(light_dir).max(0.0);
    if ndotl <= 0.0 {
        return 0.0;
    }
    let shadow = Ray {
        origin: hit.p + hit.normal * EPS * 4.0,
        dir: light_dir,
    };
    if scene.hit(&shadow, EPS, dist - EPS).is_some() {
        0.0
    } else {
        ndotl * 0.85
    }
}

fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - n * 2.0 * v.dot(n)
}

fn refract(uv: Vec3, n: Vec3, eta: f32) -> Vec3 {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_out_perp = (uv + n * cos_theta) * eta;
    let r_out_parallel = n * -(1.0 - r_out_perp.length_squared()).abs().sqrt();
    r_out_perp + r_out_parallel
}

fn schlick(cosine: f32, eta: f32) -> f32 {
    let r0 = ((1.0 - eta) / (1.0 + eta)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cosine).powi(5)
}

#[derive(Clone, Copy)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    fn union(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn hit(self, ray: &Ray, mut t_min: f32, mut t_max: f32) -> bool {
        for axis in 0..3 {
            let inv_d = 1.0 / ray.dir.axis(axis);
            let mut t0 = (self.min.axis(axis) - ray.origin.axis(axis)) * inv_d;
            let mut t1 = (self.max.axis(axis) - ray.origin.axis(axis)) * inv_d;
            if inv_d < 0.0 {
                core::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_max <= t_min {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, Default)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    const fn splat(v: f32) -> Vec3 {
        Vec3 { x: v, y: v, z: v }
    }

    fn dot(self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    fn length_squared(self) -> f32 {
        self.dot(self)
    }

    fn normalized(self) -> Vec3 {
        self / self.length().max(1.0e-8)
    }

    fn axis(self, axis: usize) -> f32 {
        match axis {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    fn min(self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }

    fn max(self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    fn lerp(self, other: Vec3, t: f32) -> Vec3 {
        self * (1.0 - t) + other * t
    }

    fn clamped(self, lo: f32, hi: f32) -> Vec3 {
        Vec3::new(
            self.x.clamp(lo, hi),
            self.y.clamp(lo, hi),
            self.z.clamp(lo, hi),
        )
    }
}

impl core::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl core::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Vec3) {
        *self = *self + rhs;
    }
}

impl core::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl core::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl core::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: f32) -> Vec3 {
        Vec3::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl core::ops::Mul<Vec3> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }
}

impl core::ops::MulAssign<Vec3> for Vec3 {
    fn mul_assign(&mut self, rhs: Vec3) {
        *self = *self * rhs;
    }
}

impl core::ops::MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl core::ops::Div<f32> for Vec3 {
    type Output = Vec3;
    fn div(self, rhs: f32) -> Vec3 {
        Vec3::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl core::ops::DivAssign<f32> for Vec3 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 0.999) * 256.0) as u8
}

fn hash_seed(x: u32, y: u32, sample: u32, frame: u32) -> u32 {
    let mut n = x
        .wrapping_mul(1973)
        .wrapping_add(y.wrapping_mul(9277))
        .wrapping_add(sample.wrapping_mul(26699))
        .wrapping_add(frame.wrapping_mul(104729));
    n ^= n << 13;
    n ^= n >> 17;
    n ^ (n << 5)
}

fn rand01(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x as f32) / (u32::MAX as f32)
}

fn random_unit(seed: u32, bounce: u32) -> Vec3 {
    let a = rand01(seed ^ bounce.wrapping_mul(17)) * core::f32::consts::TAU;
    let z = rand01(seed ^ bounce.wrapping_mul(131)) * 2.0 - 1.0;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * a.cos(), z, r * a.sin())
}
