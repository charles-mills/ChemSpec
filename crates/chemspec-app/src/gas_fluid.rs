//! Deterministic low-resolution Eulerian gas dynamics for the macroscopic view.
//!
//! Chemistry selects whether gas exists and supplies normalized visual
//! intensity. This module only advances density, temperature, and velocity
//! through reusable fluid mechanics; it never inspects chemical identity.

// Grid dimensions and the simulated time horizon are small compile-time
// constants. These casts convert already-clamped cell coordinates and step
// counts; they cannot overflow the target integer types.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use glam::Vec3;

const WIDTH: usize = 8;
const HEIGHT: usize = 12;
const DEPTH: usize = 8;
const CELL_COUNT: usize = WIDTH * HEIGHT * DEPTH;
const FIXED_DT: f32 = 1.0 / 18.0;
const MAX_SIMULATION_SECONDS: f32 = 7.0;
const PRESSURE_ITERATIONS: usize = 5;
const CONTROL_QUANTIZATION: f32 = 32.0;
const CACHE_CAPACITY: usize = 64;
const RIM_HEIGHT: f32 = 0.58;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GasFlowControls {
    pub source_strength: f32,
    pub turbulence: f32,
    pub thermal_buoyancy: f32,
    pub density_weight: f32,
    /// Stable density stratification inside the vessel. Zero produces a mixed
    /// headspace or plume; one produces a floor-hugging gravity current with a
    /// readable interface. This is a visual regime, not a chemical identity.
    pub stratification: f32,
    /// Normalized height of the retained layer's moving upper interface.
    pub layer_height: f32,
    /// Concentration pressure makes dense neighbouring cells push through the
    /// shared velocity field instead of behaving as independent particles.
    pub mixing_pressure: f32,
    /// Fraction of the vessel headspace fed by diffuse secondary sources.
    pub volume_fill: f32,
    pub drag: f32,
    pub retention: f32,
    pub wind: Vec3,
}

impl GasFlowControls {
    pub(crate) fn contained(
        source_strength: f32,
        turbulence: f32,
        heat: f32,
        pressure: f32,
        seed: u64,
    ) -> Self {
        let draft = seeded_direction(seed);
        Self {
            source_strength: source_strength.clamp(0.0, 1.0),
            turbulence: (0.20 + turbulence * 0.58 + pressure * 0.18).clamp(0.0, 1.0),
            thermal_buoyancy: (0.12 + heat * 0.62).clamp(0.0, 0.82),
            // This is a conservative visual density contrast against ambient
            // air, not an invented molecular mass.
            density_weight: 0.16,
            stratification: 0.0,
            layer_height: RIM_HEIGHT,
            mixing_pressure: (0.24 + turbulence * 0.22 + pressure * 0.16).clamp(0.0, 0.62),
            volume_fill: (0.78 + source_strength * 0.18).clamp(0.0, 0.96),
            drag: 0.22,
            retention: 0.93,
            wind: draft * (0.08 + turbulence * 0.12),
        }
    }

    /// A persistent gaseous product occupies the vessel as a continuous
    /// gravity-current layer. Formation advances the interface while heat can
    /// temporarily loft and mix it. Chemistry remains responsible for deciding
    /// that the product is gas; this constructor does not inspect its name.
    pub(crate) fn retained_product(
        source_strength: f32,
        turbulence: f32,
        heat: f32,
        pressure: f32,
        formation: f32,
        seed: u64,
    ) -> Self {
        let source_strength = source_strength.clamp(0.0, 1.0);
        let heat = heat.clamp(0.0, 1.0);
        let formation = smoother_step(formation);
        let cooling = 1.0 - heat * 0.68;
        let stratification = ((0.58 + formation * 0.34) * cooling).clamp(0.20, 0.94);
        let draft = seeded_direction(seed);
        Self {
            source_strength,
            turbulence: (0.18 + turbulence * 0.42 + pressure * 0.22).clamp(0.0, 0.84),
            thermal_buoyancy: (heat * 0.72 + pressure * 0.08).clamp(0.0, 0.82),
            // This conservative default creates visible weight without
            // claiming a species-specific density that the validated input
            // does not contain.
            density_weight: 0.13 + stratification * 0.27,
            stratification,
            layer_height: -0.72 + formation * 0.98,
            mixing_pressure: (0.18 + turbulence * 0.18 + pressure * 0.14).clamp(0.0, 0.52),
            volume_fill: (0.12 + (1.0 - stratification) * 0.34).clamp(0.10, 0.46),
            drag: 0.24 + stratification * 0.12,
            retention: 0.98,
            wind: draft * (0.035 + turbulence * 0.075),
        }
    }

    pub(crate) fn escaping(source_strength: f32, turbulence: f32, lift: f32, seed: u64) -> Self {
        let draft = seeded_direction(seed);
        Self {
            source_strength: source_strength.clamp(0.0, 1.0),
            turbulence: (0.34 + turbulence * 0.72).clamp(0.0, 1.0),
            thermal_buoyancy: (0.32 + lift * 0.64).clamp(0.0, 1.0),
            density_weight: 0.08,
            stratification: 0.0,
            layer_height: RIM_HEIGHT,
            mixing_pressure: (0.16 + turbulence * 0.18).clamp(0.0, 0.40),
            volume_fill: 0.16,
            drag: 0.13,
            retention: 0.46,
            wind: draft * (0.18 + turbulence * 0.22),
        }
    }

    fn quantized(self) -> Self {
        Self {
            source_strength: quantize_control(self.source_strength),
            turbulence: quantize_control(self.turbulence),
            thermal_buoyancy: quantize_control(self.thermal_buoyancy),
            density_weight: quantize_control(self.density_weight),
            stratification: quantize_control(self.stratification),
            layer_height: quantize_control(self.layer_height),
            mixing_pressure: quantize_control(self.mixing_pressure),
            volume_fill: quantize_control(self.volume_fill),
            drag: quantize_control(self.drag),
            retention: quantize_control(self.retention),
            wind: Vec3::new(
                quantize_control(self.wind.x),
                quantize_control(self.wind.y),
                quantize_control(self.wind.z),
            ),
        }
    }

    fn cache_components(self) -> [u32; 13] {
        [
            self.source_strength.to_bits(),
            self.turbulence.to_bits(),
            self.thermal_buoyancy.to_bits(),
            self.density_weight.to_bits(),
            self.stratification.to_bits(),
            self.layer_height.to_bits(),
            self.mixing_pressure.to_bits(),
            self.volume_fill.to_bits(),
            self.drag.to_bits(),
            self.retention.to_bits(),
            self.wind.x.to_bits(),
            self.wind.y.to_bits(),
            self.wind.z.to_bits(),
        ]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GasFluidVolume {
    density: Arc<[f32]>,
    velocity: Arc<[Vec3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GasCacheKey {
    seed: u64,
    step: u16,
    controls: [u32; 13],
}

#[derive(Debug)]
struct Solver {
    density: Vec<f32>,
    density_next: Vec<f32>,
    temperature: Vec<f32>,
    temperature_next: Vec<f32>,
    velocity: Vec<Vec3>,
    velocity_next: Vec<Vec3>,
    pressure: Vec<f32>,
    pressure_next: Vec<f32>,
    divergence: Vec<f32>,
    curl: Vec<Vec3>,
}

impl GasFluidVolume {
    pub(crate) fn simulate(seed: u64, seconds: f32, controls: GasFlowControls) -> Self {
        let target = seconds.clamp(0.0, MAX_SIMULATION_SECONDS);
        let steps = (target / FIXED_DT).round() as usize;
        let controls = controls.quantized();
        let key = GasCacheKey {
            seed,
            step: u16::try_from(steps).expect("bounded gas step count fits u16"),
            controls: controls.cache_components(),
        };
        if let Some(cached) = cached_volume(key) {
            return cached;
        }

        let mut solver = Solver::new(seed, controls);
        for step in 0..steps {
            solver.step(FIXED_DT, step as f32 * FIXED_DT, seed, controls);
        }
        insert_cached_volume(
            key,
            Self {
                density: solver.density.into(),
                velocity: solver.velocity.into(),
            },
        )
    }

    pub(crate) const fn dimensions() -> [usize; 3] {
        [WIDTH, HEIGHT, DEPTH]
    }

    pub(crate) fn density_at(&self, x: usize, y: usize, z: usize) -> f32 {
        self.density[index(x, y, z)]
    }

    pub(crate) fn velocity_at(&self, x: usize, y: usize, z: usize) -> Vec3 {
        self.velocity[index(x, y, z)]
    }

    pub(crate) fn grid_position(x: usize, y: usize, z: usize) -> Vec3 {
        Vec3::new(
            grid_axis(x, WIDTH),
            grid_axis(y, HEIGHT),
            grid_axis(z, DEPTH),
        )
    }

    #[cfg(test)]
    fn mass(&self) -> f32 {
        self.density.iter().sum()
    }

    #[cfg(test)]
    fn center_of_mass(&self) -> Vec3 {
        let mut weighted = Vec3::ZERO;
        let mut mass = 0.0;
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let density = self.density_at(x, y, z);
                    weighted += Self::grid_position(x, y, z) * density;
                    mass += density;
                }
            }
        }
        weighted / mass.max(f32::EPSILON)
    }

    #[cfg(test)]
    fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.density, &other.density) && Arc::ptr_eq(&self.velocity, &other.velocity)
    }
}

fn quantize_control(value: f32) -> f32 {
    (value * CONTROL_QUANTIZATION).round() / CONTROL_QUANTIZATION
}

fn gas_cache() -> &'static Mutex<VecDeque<(GasCacheKey, GasFluidVolume)>> {
    static CACHE: OnceLock<Mutex<VecDeque<(GasCacheKey, GasFluidVolume)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::with_capacity(CACHE_CAPACITY)))
}

fn cached_volume(key: GasCacheKey) -> Option<GasFluidVolume> {
    gas_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find_map(|(cached_key, volume)| (*cached_key == key).then(|| volume.clone()))
}

fn insert_cached_volume(key: GasCacheKey, volume: GasFluidVolume) -> GasFluidVolume {
    let mut cache = gas_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, cached)) = cache.iter().find(|(cached_key, _)| *cached_key == key) {
        return cached.clone();
    }
    if cache.len() == CACHE_CAPACITY {
        cache.pop_front();
    }
    cache.push_back((key, volume.clone()));
    volume
}

impl Solver {
    fn new(seed: u64, controls: GasFlowControls) -> Self {
        let mut solver = Self {
            density: vec![0.0; CELL_COUNT],
            density_next: vec![0.0; CELL_COUNT],
            temperature: vec![0.0; CELL_COUNT],
            temperature_next: vec![0.0; CELL_COUNT],
            velocity: vec![Vec3::ZERO; CELL_COUNT],
            velocity_next: vec![Vec3::ZERO; CELL_COUNT],
            pressure: vec![0.0; CELL_COUNT],
            pressure_next: vec![0.0; CELL_COUNT],
            divergence: vec![0.0; CELL_COUNT],
            curl: vec![Vec3::ZERO; CELL_COUNT],
        };
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let position = GasFluidVolume::grid_position(x, y, z);
                    if !inside_simulation_domain(position) {
                        continue;
                    }
                    let ellipsoid = Vec3::new(
                        position.x / 0.82,
                        (position.y + 0.08) / 0.88,
                        position.z / 0.82,
                    )
                    .length_squared();
                    if ellipsoid >= 1.0 {
                        continue;
                    }
                    let index = index(x, y, z);
                    let noise = 0.82 + hash_unit(seed, index as u32, 1) * 0.36;
                    let core = (1.0 - ellipsoid)
                        .powf((1.52 - controls.volume_fill * 0.92).clamp(0.52, 1.52));
                    let ambient_fill = controls.volume_fill * 0.16;
                    let mixed_density = core * (1.0 - ambient_fill) + ambient_fill;
                    let interface = layer_interface(position, seed, controls);
                    let layer_density = retained_layer_density(position, interface);
                    let entrainment = retained_entrainment(position.y, interface);
                    solver.density[index] = controls.source_strength
                        * (mixed_density * (1.0 - controls.stratification) * entrainment
                            + layer_density * controls.stratification)
                        * noise;
                    solver.temperature[index] = controls.thermal_buoyancy
                        * (1.0 - ellipsoid)
                        * 0.55
                        * (1.0 - controls.stratification * 0.36);
                    solver.velocity[index] = layer_velocity(position, 0.0, seed, controls);
                }
            }
        }
        solver
    }

    fn step(&mut self, dt: f32, time: f32, seed: u64, controls: GasFlowControls) {
        if dt <= f32::EPSILON {
            return;
        }
        self.inject_source(dt, time, seed, controls);
        self.apply_forces(dt, time, seed, controls);
        advect_vec3(&self.velocity, &self.velocity, &mut self.velocity_next, dt);
        std::mem::swap(&mut self.velocity, &mut self.velocity_next);
        apply_velocity_boundary(&mut self.velocity);
        self.add_vorticity_confinement(dt, controls.turbulence);
        self.project_velocity();
        advect_scalar(&self.density, &self.velocity, &mut self.density_next, dt);
        advect_scalar(
            &self.temperature,
            &self.velocity,
            &mut self.temperature_next,
            dt,
        );
        std::mem::swap(&mut self.density, &mut self.density_next);
        std::mem::swap(&mut self.temperature, &mut self.temperature_next);
        diffuse_scalar(
            &self.density,
            &mut self.density_next,
            dt,
            (0.38 + controls.volume_fill * 0.58) * (1.0 - controls.stratification * 0.62),
        );
        diffuse_scalar(
            &self.temperature,
            &mut self.temperature_next,
            dt,
            0.22 + controls.turbulence * 0.24,
        );
        std::mem::swap(&mut self.density, &mut self.density_next);
        std::mem::swap(&mut self.temperature, &mut self.temperature_next);
        self.dissipate(dt, controls);
    }

    fn inject_source(&mut self, dt: f32, time: f32, seed: u64, controls: GasFlowControls) {
        let pulse = 0.72 + 0.28 * (time * 2.7 + hash_unit(seed, 0, 9) * 6.0).sin().abs();
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let position = GasFluidVolume::grid_position(x, y, z);
                    let radial = position.x.hypot(position.z);
                    if position.y >= RIM_HEIGHT {
                        continue;
                    }
                    let radius = vessel_radius(position.y);
                    let core_radial = (1.0 - radial / 0.36).max(0.0);
                    let core_vertical = (1.0 - (position.y + 0.48).abs() / 0.34).max(0.0);
                    let core = core_radial * core_vertical;
                    let wall_distance = (1.0 - radial / radius.max(0.01)).clamp(0.0, 1.0);
                    let height = ((position.y + 0.88) / (RIM_HEIGHT + 0.88)).clamp(0.0, 1.0);
                    let distributed = controls.volume_fill
                        * (0.18 + wall_distance * 0.42)
                        * (0.72 + (height * std::f32::consts::PI).sin() * 0.28);
                    let interface = layer_interface(position, seed.rotate_left(13), controls);
                    let layer_source =
                        retained_layer_density(position, interface) * (0.64 + wall_distance * 0.36);
                    let falloff = (core + distributed * 0.34)
                        * (1.0 - controls.stratification)
                        * retained_entrainment(position.y, interface)
                        + layer_source * controls.stratification;
                    if falloff <= f32::EPSILON {
                        continue;
                    }
                    let index = index(x, y, z);
                    let spatial_pulse = 0.86 + hash_unit(seed, index as u32, 10) * 0.14;
                    let injection =
                        controls.source_strength * falloff * pulse * spatial_pulse * dt * 0.78;
                    self.density[index] = (self.density[index] + injection).min(1.65);
                    self.temperature[index] =
                        (self.temperature[index] + injection * controls.thermal_buoyancy).min(1.0);
                }
            }
        }
    }

    fn apply_forces(&mut self, dt: f32, time: f32, seed: u64, controls: GasFlowControls) {
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let position = GasFluidVolume::grid_position(x, y, z);
                    if !inside_simulation_domain(position) {
                        continue;
                    }
                    let index = index(x, y, z);
                    let density = self.density[index];
                    let temperature = self.temperature[index];
                    let buoyancy =
                        temperature * controls.thermal_buoyancy - density * controls.density_weight;
                    let concentration_gradient = Vec3::new(
                        scalar_neighbor(&self.density, x, y, z, 1, 0, 0)
                            - scalar_neighbor(&self.density, x, y, z, -1, 0, 0),
                        scalar_neighbor(&self.density, x, y, z, 0, 1, 0)
                            - scalar_neighbor(&self.density, x, y, z, 0, -1, 0),
                        scalar_neighbor(&self.density, x, y, z, 0, 0, 1)
                            - scalar_neighbor(&self.density, x, y, z, 0, 0, -1),
                    ) * 0.5;
                    let turbulence = curl_wind(position, time, seed) * controls.turbulence * 0.34;
                    let wall_damping = wall_proximity(position);
                    let draft = controls.wind * (0.42 + wall_damping * 0.58);
                    let concentration_pressure = -concentration_gradient * controls.mixing_pressure;
                    let interface_distance = (position.y - controls.layer_height).abs();
                    let interface_band = (-interface_distance * interface_distance / 0.055).exp();
                    let gravity_current = Vec3::new(position.x, 0.0, position.z)
                        .normalize_or_zero()
                        * density
                        * controls.stratification
                        * (0.08 + interface_band * 0.13);
                    self.velocity[index] += (Vec3::Y * buoyancy
                        + concentration_pressure
                        + turbulence
                        + draft
                        + gravity_current)
                        * dt;
                    // A cooled density layer suppresses vertical oscillation
                    // while retaining horizontal rolling and entrainment at
                    // its irregular upper interface.
                    self.velocity[index].y /= 1.0
                        + controls.stratification
                            * density
                            * dt
                            * (0.82 + (1.0 - interface_band) * 0.48);
                    // Tangential momentum survives contact, but the thin
                    // stationary layer next to the glass removes energy.
                    let wall_drag = 1.0 + (1.0 - wall_damping) * dt * 1.8;
                    self.velocity[index] /= wall_drag;
                    self.velocity[index] *= 1.0 / (1.0 + controls.drag * dt);
                }
            }
        }
    }

    fn add_vorticity_confinement(&mut self, dt: f32, strength: f32) {
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if !fluid_cell(x, y, z) {
                        continue;
                    }
                    let dx = velocity_neighbor(&self.velocity, x, y, z, 1, 0, 0)
                        - velocity_neighbor(&self.velocity, x, y, z, -1, 0, 0);
                    let dy = velocity_neighbor(&self.velocity, x, y, z, 0, 1, 0)
                        - velocity_neighbor(&self.velocity, x, y, z, 0, -1, 0);
                    let dz = velocity_neighbor(&self.velocity, x, y, z, 0, 0, 1)
                        - velocity_neighbor(&self.velocity, x, y, z, 0, 0, -1);
                    self.curl[index(x, y, z)] = Vec3::new(
                        (dy.z - dz.y) * 0.5,
                        (dz.x - dx.z) * 0.5,
                        (dx.y - dy.x) * 0.5,
                    );
                }
            }
        }
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if !fluid_cell(x, y, z) {
                        continue;
                    }
                    let gradient = Vec3::new(
                        curl_magnitude_neighbor(&self.curl, x, y, z, 1, 0, 0)
                            - curl_magnitude_neighbor(&self.curl, x, y, z, -1, 0, 0),
                        curl_magnitude_neighbor(&self.curl, x, y, z, 0, 1, 0)
                            - curl_magnitude_neighbor(&self.curl, x, y, z, 0, -1, 0),
                        curl_magnitude_neighbor(&self.curl, x, y, z, 0, 0, 1)
                            - curl_magnitude_neighbor(&self.curl, x, y, z, 0, 0, -1),
                    )
                    .normalize_or_zero();
                    let index = index(x, y, z);
                    self.velocity[index] += gradient.cross(self.curl[index]) * strength * dt * 0.32;
                }
            }
        }
        apply_velocity_boundary(&mut self.velocity);
    }

    fn project_velocity(&mut self) {
        self.pressure.fill(0.0);
        self.pressure_next.fill(0.0);
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if !fluid_cell(x, y, z) {
                        continue;
                    }
                    let right = velocity_neighbor(&self.velocity, x, y, z, 1, 0, 0).x;
                    let left = velocity_neighbor(&self.velocity, x, y, z, -1, 0, 0).x;
                    let up = velocity_neighbor(&self.velocity, x, y, z, 0, 1, 0).y;
                    let down = velocity_neighbor(&self.velocity, x, y, z, 0, -1, 0).y;
                    let front = velocity_neighbor(&self.velocity, x, y, z, 0, 0, 1).z;
                    let back = velocity_neighbor(&self.velocity, x, y, z, 0, 0, -1).z;
                    self.divergence[index(x, y, z)] =
                        -0.5 * (right - left + up - down + front - back);
                }
            }
        }
        for _ in 0..PRESSURE_ITERATIONS {
            for z in 0..DEPTH {
                for y in 0..HEIGHT {
                    for x in 0..WIDTH {
                        if !fluid_cell(x, y, z) {
                            continue;
                        }
                        let sum = scalar_neighbor(&self.pressure, x, y, z, 1, 0, 0)
                            + scalar_neighbor(&self.pressure, x, y, z, -1, 0, 0)
                            + scalar_neighbor(&self.pressure, x, y, z, 0, 1, 0)
                            + scalar_neighbor(&self.pressure, x, y, z, 0, -1, 0)
                            + scalar_neighbor(&self.pressure, x, y, z, 0, 0, 1)
                            + scalar_neighbor(&self.pressure, x, y, z, 0, 0, -1);
                        self.pressure_next[index(x, y, z)] =
                            (self.divergence[index(x, y, z)] + sum) / 6.0;
                    }
                }
            }
            std::mem::swap(&mut self.pressure, &mut self.pressure_next);
        }
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if !fluid_cell(x, y, z) {
                        continue;
                    }
                    let gradient = Vec3::new(
                        scalar_neighbor(&self.pressure, x, y, z, 1, 0, 0)
                            - scalar_neighbor(&self.pressure, x, y, z, -1, 0, 0),
                        scalar_neighbor(&self.pressure, x, y, z, 0, 1, 0)
                            - scalar_neighbor(&self.pressure, x, y, z, 0, -1, 0),
                        scalar_neighbor(&self.pressure, x, y, z, 0, 0, 1)
                            - scalar_neighbor(&self.pressure, x, y, z, 0, 0, -1),
                    ) * 0.5;
                    self.velocity[index(x, y, z)] -= gradient;
                }
            }
        }
        apply_velocity_boundary(&mut self.velocity);
    }

    fn dissipate(&mut self, dt: f32, controls: GasFlowControls) {
        let density_decay = 1.0 / (1.0 + dt * (0.045 + (1.0 - controls.retention) * 0.48));
        let temperature_decay = 1.0 / (1.0 + dt * 0.38);
        for z in 0..DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let index = index(x, y, z);
                    if !fluid_cell(x, y, z) {
                        self.density[index] = 0.0;
                        self.temperature[index] = 0.0;
                        continue;
                    }
                    let position = GasFluidVolume::grid_position(x, y, z);
                    let escaped = smoother_step(
                        ((position.y - RIM_HEIGHT) / (1.0 - RIM_HEIGHT)).clamp(0.0, 1.0),
                    );
                    let escape_decay =
                        1.0 / (1.0 + dt * escaped * (1.0 - controls.retention) * 2.8);
                    self.density[index] *= density_decay * escape_decay;
                    self.temperature[index] *= temperature_decay;
                }
            }
        }
    }
}

fn advect_scalar(source: &[f32], velocity: &[Vec3], target: &mut [f32], dt: f32) {
    for z in 0..DEPTH {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let index = index(x, y, z);
                let position = GasFluidVolume::grid_position(x, y, z);
                if !inside_simulation_domain(position) {
                    target[index] = 0.0;
                    continue;
                }
                let previous = constrain_backtrace(position - velocity[index] * dt);
                target[index] = sample_scalar(source, previous);
            }
        }
    }
}

fn advect_vec3(source: &[Vec3], velocity: &[Vec3], target: &mut [Vec3], dt: f32) {
    for z in 0..DEPTH {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let index = index(x, y, z);
                let position = GasFluidVolume::grid_position(x, y, z);
                if !inside_simulation_domain(position) {
                    target[index] = Vec3::ZERO;
                    continue;
                }
                let previous = constrain_backtrace(position - velocity[index] * dt);
                target[index] = sample_vec3(source, previous);
            }
        }
    }
}

fn diffuse_scalar(source: &[f32], target: &mut [f32], dt: f32, rate: f32) {
    let response = 1.0 - (-rate.max(0.0) * dt).exp();
    for z in 0..DEPTH {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let cell = index(x, y, z);
                if !fluid_cell(x, y, z) {
                    target[cell] = 0.0;
                    continue;
                }
                let neighbour_average = (scalar_neighbor(source, x, y, z, 1, 0, 0)
                    + scalar_neighbor(source, x, y, z, -1, 0, 0)
                    + scalar_neighbor(source, x, y, z, 0, 1, 0)
                    + scalar_neighbor(source, x, y, z, 0, -1, 0)
                    + scalar_neighbor(source, x, y, z, 0, 0, 1)
                    + scalar_neighbor(source, x, y, z, 0, 0, -1))
                    / 6.0;
                target[cell] = source[cell] + (neighbour_average - source[cell]) * response;
            }
        }
    }
}

fn sample_scalar(field: &[f32], position: Vec3) -> f32 {
    let coordinate = grid_coordinate(position);
    let base = coordinate.floor();
    let fraction = coordinate - base;
    let x0 = base.x as usize;
    let y0 = base.y as usize;
    let z0 = base.z as usize;
    let x1 = (x0 + 1).min(WIDTH - 1);
    let y1 = (y0 + 1).min(HEIGHT - 1);
    let z1 = (z0 + 1).min(DEPTH - 1);
    trilinear(
        field[index(x0, y0, z0)],
        field[index(x1, y0, z0)],
        field[index(x0, y1, z0)],
        field[index(x1, y1, z0)],
        field[index(x0, y0, z1)],
        field[index(x1, y0, z1)],
        field[index(x0, y1, z1)],
        field[index(x1, y1, z1)],
        fraction,
    )
}

fn sample_vec3(field: &[Vec3], position: Vec3) -> Vec3 {
    let coordinate = grid_coordinate(position);
    let base = coordinate.floor();
    let fraction = coordinate - base;
    let x0 = base.x as usize;
    let y0 = base.y as usize;
    let z0 = base.z as usize;
    let x1 = (x0 + 1).min(WIDTH - 1);
    let y1 = (y0 + 1).min(HEIGHT - 1);
    let z1 = (z0 + 1).min(DEPTH - 1);
    trilinear(
        field[index(x0, y0, z0)],
        field[index(x1, y0, z0)],
        field[index(x0, y1, z0)],
        field[index(x1, y1, z0)],
        field[index(x0, y0, z1)],
        field[index(x1, y0, z1)],
        field[index(x0, y1, z1)],
        field[index(x1, y1, z1)],
        fraction,
    )
}

#[allow(clippy::too_many_arguments)]
fn trilinear<T>(
    c000: T,
    c100: T,
    c010: T,
    c110: T,
    c001: T,
    c101: T,
    c011: T,
    c111: T,
    fraction: Vec3,
) -> T
where
    T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>,
{
    let x00 = c000 * (1.0 - fraction.x) + c100 * fraction.x;
    let x10 = c010 * (1.0 - fraction.x) + c110 * fraction.x;
    let x01 = c001 * (1.0 - fraction.x) + c101 * fraction.x;
    let x11 = c011 * (1.0 - fraction.x) + c111 * fraction.x;
    let y0 = x00 * (1.0 - fraction.y) + x10 * fraction.y;
    let y1 = x01 * (1.0 - fraction.y) + x11 * fraction.y;
    y0 * (1.0 - fraction.z) + y1 * fraction.z
}

fn apply_velocity_boundary(velocity: &mut [Vec3]) {
    for z in 0..DEPTH {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let index = index(x, y, z);
                if !fluid_cell(x, y, z) {
                    velocity[index] = Vec3::ZERO;
                    continue;
                }
                if !offset_fluid_cell(x, y, z, -1, 0, 0) && velocity[index].x < 0.0
                    || !offset_fluid_cell(x, y, z, 1, 0, 0) && velocity[index].x > 0.0
                {
                    velocity[index].x = 0.0;
                }
                if !offset_fluid_cell(x, y, z, 0, -1, 0) && velocity[index].y < 0.0
                    || !offset_fluid_cell(x, y, z, 0, 1, 0) && velocity[index].y > 0.0
                {
                    velocity[index].y = 0.0;
                }
                if !offset_fluid_cell(x, y, z, 0, 0, -1) && velocity[index].z < 0.0
                    || !offset_fluid_cell(x, y, z, 0, 0, 1) && velocity[index].z > 0.0
                {
                    velocity[index].z = 0.0;
                }
            }
        }
    }
}

fn constrain_backtrace(position: Vec3) -> Vec3 {
    let mut constrained = position.clamp(Vec3::splat(-0.999), Vec3::splat(0.999));
    if constrained.y <= RIM_HEIGHT {
        let radius = vessel_radius(constrained.y);
        let radial = constrained.x.hypot(constrained.z);
        if radial > radius {
            let scale = radius / radial.max(f32::EPSILON);
            constrained.x *= scale;
            constrained.z *= scale;
        }
    }
    constrained
}

fn inside_simulation_domain(position: Vec3) -> bool {
    if position.y <= -0.96 || position.y >= 0.99 {
        return false;
    }
    if position.y <= RIM_HEIGHT {
        position.x.hypot(position.z) <= vessel_radius(position.y)
    } else {
        position.x.abs() < 0.99 && position.z.abs() < 0.99
    }
}

fn vessel_radius(y: f32) -> f32 {
    let height = ((y + 0.96) / (RIM_HEIGHT + 0.96)).clamp(0.0, 1.0);
    0.74 + height * 0.18
}

fn wall_proximity(position: Vec3) -> f32 {
    if position.y > RIM_HEIGHT {
        return 1.0;
    }
    let distance = vessel_radius(position.y) - position.x.hypot(position.z);
    (distance / 0.28).clamp(0.0, 1.0)
}

fn fluid_cell(x: usize, y: usize, z: usize) -> bool {
    inside_simulation_domain(GasFluidVolume::grid_position(x, y, z))
}

fn offset_fluid_cell(x: usize, y: usize, z: usize, dx: isize, dy: isize, dz: isize) -> bool {
    let Some(x) = x.checked_add_signed(dx) else {
        return false;
    };
    let Some(y) = y.checked_add_signed(dy) else {
        return false;
    };
    let Some(z) = z.checked_add_signed(dz) else {
        return false;
    };
    x < WIDTH && y < HEIGHT && z < DEPTH && fluid_cell(x, y, z)
}

fn velocity_neighbor(
    field: &[Vec3],
    x: usize,
    y: usize,
    z: usize,
    dx: isize,
    dy: isize,
    dz: isize,
) -> Vec3 {
    if offset_fluid_cell(x, y, z, dx, dy, dz) {
        field[index(
            x.checked_add_signed(dx).unwrap_or(x),
            y.checked_add_signed(dy).unwrap_or(y),
            z.checked_add_signed(dz).unwrap_or(z),
        )]
    } else {
        Vec3::ZERO
    }
}

fn scalar_neighbor(
    field: &[f32],
    x: usize,
    y: usize,
    z: usize,
    dx: isize,
    dy: isize,
    dz: isize,
) -> f32 {
    if offset_fluid_cell(x, y, z, dx, dy, dz) {
        field[index(
            x.checked_add_signed(dx).unwrap_or(x),
            y.checked_add_signed(dy).unwrap_or(y),
            z.checked_add_signed(dz).unwrap_or(z),
        )]
    } else {
        field[index(x, y, z)]
    }
}

fn curl_magnitude_neighbor(
    field: &[Vec3],
    x: usize,
    y: usize,
    z: usize,
    dx: isize,
    dy: isize,
    dz: isize,
) -> f32 {
    if offset_fluid_cell(x, y, z, dx, dy, dz) {
        field[index(
            x.checked_add_signed(dx).unwrap_or(x),
            y.checked_add_signed(dy).unwrap_or(y),
            z.checked_add_signed(dz).unwrap_or(z),
        )]
        .length()
    } else {
        field[index(x, y, z)].length()
    }
}

fn grid_coordinate(position: Vec3) -> Vec3 {
    ((position.clamp(Vec3::splat(-1.0), Vec3::splat(1.0)) + Vec3::ONE) * 0.5)
        * Vec3::new((WIDTH - 1) as f32, (HEIGHT - 1) as f32, (DEPTH - 1) as f32)
}

fn grid_axis(index: usize, extent: usize) -> f32 {
    index as f32 / (extent - 1) as f32 * 2.0 - 1.0
}

const fn index(x: usize, y: usize, z: usize) -> usize {
    (z * HEIGHT + y) * WIDTH + x
}

fn layer_interface(position: Vec3, seed: u64, controls: GasFlowControls) -> f32 {
    let broad_roll = (position.x * 2.7 + seed_phase(seed, 41)).sin()
        * (position.z * 2.2 + seed_phase(seed, 42)).cos();
    let fine_roll = (position.x * 5.3 - position.z * 4.7 + seed_phase(seed, 43)).sin();
    controls.layer_height
        + controls.stratification * controls.turbulence * (broad_roll * 0.075 + fine_roll * 0.026)
}

fn retained_layer_density(position: Vec3, interface: f32) -> f32 {
    let radial = position.x.hypot(position.z);
    let radius = vessel_radius(position.y).max(0.01);
    let wall_fill = smoother_step(((radius - radial) / 0.18).clamp(0.0, 1.0));
    let lower_boundary = smoother_step(((position.y + 0.96) / 0.16).clamp(0.0, 1.0));
    let interface_fill =
        1.0 - smoother_step(((position.y - interface) / 0.18 + 0.5).clamp(0.0, 1.0));
    wall_fill * lower_boundary * interface_fill
}

fn retained_entrainment(y: f32, interface: f32) -> f32 {
    1.0 - smoother_step(((y - interface) / 0.36).clamp(0.0, 1.0))
}

fn layer_velocity(position: Vec3, time: f32, seed: u64, controls: GasFlowControls) -> Vec3 {
    let rolling = curl_wind(position, time, seed) * controls.turbulence * 0.045;
    let radial = Vec3::new(position.x, 0.0, position.z).normalize_or_zero();
    let interface_distance = (position.y - controls.layer_height).abs();
    let interface_current =
        (-interface_distance * interface_distance / 0.065).exp() * controls.stratification;
    rolling + radial * interface_current * (0.012 + controls.turbulence * 0.018)
}

fn curl_wind(position: Vec3, time: f32, seed: u64) -> Vec3 {
    let phase = Vec3::new(
        hash_unit(seed, 0, 21),
        hash_unit(seed, 0, 22),
        hash_unit(seed, 0, 23),
    ) * std::f32::consts::TAU;
    let octave = |frequency: f32, speed: f32, phase: Vec3| {
        Vec3::new(
            -(position.z * frequency + phase.y + time * speed).cos(),
            -(position.x * frequency + phase.z - time * speed * 0.71).cos() * 0.42,
            -(position.y * frequency + phase.x + time * speed * 0.83).cos(),
        )
    };
    let rotated_phase = Vec3::new(phase.y, phase.z, phase.x);
    (octave(2.15, 0.73, phase) + octave(4.70, -0.39, rotated_phase) * 0.34).normalize_or_zero()
}

fn seed_phase(seed: u64, lane: u32) -> f32 {
    hash_unit(seed, 0, lane) * std::f32::consts::TAU
}

fn seeded_direction(seed: u64) -> Vec3 {
    Vec3::new(
        hash_unit(seed, 0, 31) - 0.5,
        (hash_unit(seed, 0, 32) - 0.5) * 0.18,
        hash_unit(seed, 0, 33) - 0.5,
    )
    .normalize_or_zero()
}

fn hash_unit(seed: u64, index: u32, lane: u32) -> f32 {
    let mut value = seed
        ^ u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(lane).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    let bits = u32::try_from(value >> 40).unwrap_or(u32::MAX);
    bits as f32 / 16_777_215.0
}

fn smoother_step(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_COUNT, GasFlowControls, GasFluidVolume, HEIGHT, RIM_HEIGHT, WIDTH, diffuse_scalar,
        fluid_cell, index, inside_simulation_domain,
    };
    use glam::Vec3;

    fn controls(seed: u64) -> GasFlowControls {
        GasFlowControls::contained(0.82, 0.58, 0.16, 0.18, seed)
    }

    #[test]
    fn fixed_step_gas_is_byte_deterministic() {
        let first = GasFluidVolume::simulate(42, 2.4, controls(42));
        let repeated = GasFluidVolume::simulate(42, 2.4, controls(42));
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(first.density.as_ref()),
            bytemuck::cast_slice::<f32, u8>(repeated.density.as_ref())
        );
        assert_eq!(first.velocity, repeated.velocity);
        assert!(
            first.shares_storage_with(&repeated),
            "an unchanged fixed step should reuse its cached fluid arrays"
        );
    }

    #[test]
    fn redraws_within_one_fixed_step_reuse_the_quantized_fluid_state() {
        let controls = controls(144);
        let first = GasFluidVolume::simulate(144, 2.401, controls);
        let same_step = GasFluidVolume::simulate(
            144,
            2.409,
            GasFlowControls {
                source_strength: controls.source_strength + 0.001,
                ..controls
            },
        );
        let next_step = GasFluidVolume::simulate(144, 2.46, controls);

        assert!(first.shares_storage_with(&same_step));
        assert!(!first.shares_storage_with(&next_step));
    }

    #[test]
    fn contained_gas_respects_sides_and_floor_below_the_open_rim() {
        let volume = GasFluidVolume::simulate(91, 3.2, controls(91));
        for z in 0..super::DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let position = GasFluidVolume::grid_position(x, y, z);
                    if !inside_simulation_domain(position) {
                        assert!(volume.density_at(x, y, z) <= f32::EPSILON);
                        assert_eq!(volume.velocity_at(x, y, z), Vec3::ZERO);
                    }
                }
            }
        }
    }

    #[test]
    fn contained_gas_fills_the_beaker_headspace_as_one_dense_volume() {
        let volume = GasFluidVolume::simulate(61, 3.2, controls(61));
        let mut occupied = 0_u32;
        let mut available = 0_u32;
        for z in 0..super::DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let position = GasFluidVolume::grid_position(x, y, z);
                    if fluid_cell(x, y, z) && position.y < RIM_HEIGHT {
                        available += 1;
                        if volume.density_at(x, y, z) > 0.035 {
                            occupied += 1;
                        }
                    }
                }
            }
        }
        let fill = occupied as f32 / available as f32;
        assert!(
            fill > 0.72,
            "dense gas should fill the vessel headspace instead of forming sparse particles: {fill}"
        );
    }

    #[test]
    fn neighbouring_density_cells_mix_without_crossing_solid_boundaries() {
        let mut source = vec![0.0; CELL_COUNT];
        let mut mixed = vec![0.0; CELL_COUNT];
        let centre = (WIDTH / 2, HEIGHT / 2, super::DEPTH / 2);
        source[index(centre.0, centre.1, centre.2)] = 1.0;

        diffuse_scalar(&source, &mut mixed, 1.0 / 18.0, 0.96);

        assert!(mixed[index(centre.0 + 1, centre.1, centre.2)] > 0.0);
        assert!(mixed[index(centre.0, centre.1 + 1, centre.2)] > 0.0);
        assert!(mixed[index(centre.0, centre.1, centre.2)] < 1.0);
        for z in 0..super::DEPTH {
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    if !fluid_cell(x, y, z) {
                        assert!(mixed[index(x, y, z)].abs() <= f32::EPSILON);
                    }
                }
            }
        }
    }

    #[test]
    fn density_contrast_gives_gas_weight_while_heat_restores_buoyancy() {
        let seed = 73;
        let weighted = GasFluidVolume::simulate(
            seed,
            2.8,
            GasFlowControls {
                thermal_buoyancy: 0.0,
                density_weight: 0.34,
                wind: Vec3::ZERO,
                ..controls(seed)
            },
        );
        let hot = GasFluidVolume::simulate(
            seed,
            2.8,
            GasFlowControls {
                thermal_buoyancy: 0.92,
                density_weight: 0.04,
                wind: Vec3::ZERO,
                ..controls(seed)
            },
        );
        assert!(weighted.center_of_mass().y < hot.center_of_mass().y);
        assert!(weighted.mass() > 0.1 && hot.mass() > 0.1);
    }

    #[test]
    fn retained_product_forms_a_low_gravity_current_with_a_readable_interface() {
        let seed = 0x6c61_7965_7267_6173;
        let controls = GasFlowControls::retained_product(0.86, 0.48, 0.0, 0.12, 0.82, seed);
        let retained = GasFluidVolume::simulate(seed, 3.1, controls);
        let mixed = GasFluidVolume::simulate(
            seed,
            3.1,
            GasFlowControls::contained(0.86, 0.48, 0.0, 0.12, seed),
        );
        let (below, above) = (0..super::DEPTH)
            .flat_map(|z| (0..HEIGHT).flat_map(move |y| (0..WIDTH).map(move |x| (x, y, z))))
            .fold((0.0, 0.0), |(below, above), (x, y, z)| {
                let density = retained.density_at(x, y, z);
                if GasFluidVolume::grid_position(x, y, z).y <= controls.layer_height + 0.10 {
                    (below + density, above)
                } else {
                    (below, above + density)
                }
            });

        assert!(
            retained.center_of_mass().y + 0.16 < mixed.center_of_mass().y,
            "retained product gas should settle below a mixed headspace"
        );
        assert!(
            below > above * 2.4,
            "the retained volume should have a clear, irregular upper interface: below={below}, above={above}"
        );
        assert!(above > 0.01, "entrainment should keep the interface alive");
    }

    #[test]
    fn retained_layer_rises_as_generic_product_formation_advances() {
        let seed = 0x696e_7465_7266_6163;
        let early_controls = GasFlowControls::retained_product(0.82, 0.42, 0.0, 0.08, 0.18, seed);
        let late_controls = GasFlowControls::retained_product(0.82, 0.42, 0.0, 0.08, 0.92, seed);
        let early = GasFluidVolume::simulate(seed, 2.4, early_controls);
        let late = GasFluidVolume::simulate(seed, 2.4, late_controls);
        let occupied_top = |volume: &GasFluidVolume| {
            (0..super::DEPTH)
                .flat_map(|z| (0..HEIGHT).flat_map(move |y| (0..WIDTH).map(move |x| (x, y, z))))
                .filter(|&(x, y, z)| volume.density_at(x, y, z) > 0.045)
                .map(|(x, y, z)| GasFluidVolume::grid_position(x, y, z).y)
                .fold(-1.0, f32::max)
        };

        assert!(late_controls.layer_height > early_controls.layer_height + 0.55);
        assert!(
            occupied_top(&late) > occupied_top(&early) + 0.30,
            "the density interface should fill upward instead of scaling a cloud uniformly"
        );
        assert!(late.mass() > early.mass() * 1.25);
    }

    #[test]
    fn directional_draft_advects_density_without_crossing_the_vessel() {
        let seed = 27;
        let left = GasFluidVolume::simulate(
            seed,
            2.1,
            GasFlowControls {
                wind: Vec3::new(-0.42, 0.0, 0.0),
                ..controls(seed)
            },
        );
        let right = GasFluidVolume::simulate(
            seed,
            2.1,
            GasFlowControls {
                wind: Vec3::new(0.42, 0.0, 0.0),
                ..controls(seed)
            },
        );
        assert!(left.center_of_mass().x < right.center_of_mass().x);
    }

    #[test]
    fn escaping_gas_crosses_the_open_rim_and_remains_a_dissipating_volume() {
        let seed = 109;
        let volume =
            GasFluidVolume::simulate(seed, 4.0, GasFlowControls::escaping(0.92, 0.72, 0.94, seed));
        let mass_above_rim = (0..super::DEPTH)
            .flat_map(|z| (0..HEIGHT).flat_map(move |y| (0..WIDTH).map(move |x| (x, y, z))))
            .filter(|&(x, y, z)| GasFluidVolume::grid_position(x, y, z).y > RIM_HEIGHT)
            .map(|(x, y, z)| volume.density_at(x, y, z))
            .sum::<f32>();
        assert!(
            mass_above_rim > 0.01,
            "buoyant gas must pass through the vessel's open rim"
        );
        assert!(
            mass_above_rim < volume.mass(),
            "the escaping field should still retain a coherent source volume"
        );
    }
}
