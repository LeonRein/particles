use std::time::Duration;

use rand::prelude::*;

pub struct Particles {
    pub particles: Vec<Particle>,
}

impl Particles {
    pub fn new(n: usize, width: usize, height: usize) -> Self {
        let mut particles_vec = Vec::<Particle>::with_capacity(n);
        for _ in 0..n {
            particles_vec.push(Particle::new_random(width, height));
        }
        Self {
            particles: particles_vec,
        }
    }

    #[inline(never)]
    pub fn update(
        &mut self,
        frametime: &Duration,
        mouse_pos: Option<(f32, f32)>,
        mouse_down: bool,
    ) {
        let time_norm = frametime.as_micros() as f32 / 16666.0;
        let fric_norm = f32::powf(0.999, time_norm);
        let grav_norm = 3.0 * time_norm;

        self.particles.iter_mut().for_each(|particle| {
            if mouse_down {
                if let Some((x, y)) = mouse_pos {
                    particle.apply_grav(x, y, grav_norm);
                }
            }

            particle.apply_fric(fric_norm);

            particle.x += particle.dx;
            particle.y += particle.dy;
        });
    }
}

pub struct Particle {
    pub x: f32,
    pub y: f32,
    dx: f32,
    dy: f32,
}

impl Particle {
    // pub fn new(x: f32, y: f32, dx: f32, dy: f32) -> Self {
    //     Self { x, y, dx, dy }
    // }

    pub fn new_random(width: usize, height: usize) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            x: rng.gen_range(0.0..width as f32),
            y: rng.gen_range(0.0..height as f32),
            dx: 0.0,
            dy: 0.0,
        }
    }

    #[inline(always)]
    pub fn apply_grav(&mut self, x: f32, y: f32, grav_norm: f32) {
        let dx = self.x - x;
        let dy = self.y - y;
        let dist_sqr = f32::sqrt(dx * dx + dy * dy);

        self.dx -= grav_norm * dx / dist_sqr;
        self.dy -= grav_norm * dy / dist_sqr;
    }

    pub fn apply_fric(&mut self, fric_norm: f32) {
        self.dx *= fric_norm;
        self.dy *= fric_norm;
    }
}
