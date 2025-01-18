use std::time::Duration;

use crate::scoped_threadpool::Pool;
use rand::Rng;

pub struct Particles<'a> {
    pub particles: Vec<Particle>,
    threadpool: &'a Pool,
}

impl<'a> Particles<'a> {
    pub fn new(threadpool: &'a Pool) -> Self {
        Self {
            particles: Vec::new(),
            threadpool,
        }
    }

    pub fn add_particles(&mut self, n: usize, width: u32, height: u32) {
        for _ in 0..n {
            self.particles.push(Particle::new_random(width, height));
        }
    }

    pub fn reset(&mut self, n: usize, width: u32, height: u32) {
        self.particles.clear();
        self.add_particles(n, width, height);
    }

    #[inline(never)]
    pub fn update(&mut self, frametime: &Duration, mouse_pos: (f32, f32), mouse_down: bool) {
        let time_norm = frametime.as_micros() as f32 / 16666.0;
        let fric_norm = f32::powf(0.988, time_norm);
        let grav_norm = 1.0 * time_norm;

        let particles_chunk_len = usize::max(
            self.particles.len() / self.threadpool.thread_count() as usize / 10,
            1,
        );

        let particles_chunks = self.particles.chunks_mut(particles_chunk_len);

        self.threadpool.scoped(|scope| {
            for particles_chunk in particles_chunks {
                scope.execute(move |_| {
                    for particle in particles_chunk {
                        particle.apply_grav(mouse_pos.0, mouse_pos.1, mouse_down, grav_norm);

                        particle.apply_fric(fric_norm);

                        particle.x += particle.dx * time_norm;
                        particle.y += particle.dy * time_norm;
                    }
                });
            }
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
    pub fn new_random(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            x: rng.gen_range(0.0..width as f32),
            y: rng.gen_range(0.0..height as f32),
            dx: 0.0,
            dy: 0.0,
        }
    }

    #[inline(always)]
    pub fn apply_grav(&mut self, x: f32, y: f32, mouse_down: bool, grav_norm: f32) {
        let dx = self.x - x;
        let dy = self.y - y;
        let dist_sqr = f32::sqrt(dx * dx + dy * dy);

        self.dx -= mouse_down as u32 as f32 * grav_norm * dx / dist_sqr;
        self.dy -= mouse_down as u32 as f32 * grav_norm * dy / dist_sqr;
    }

    pub fn apply_fric(&mut self, fric_norm: f32) {
        self.dx *= fric_norm;
        self.dy *= fric_norm;
    }
}
