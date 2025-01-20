use std::{
    simd::{StdFloat, f32x64},
    time::Duration,
};

type F32s = f32x64;

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

    pub fn len(&self) -> usize {
        self.particles.len() * F32s::LEN
    }

    #[inline(never)]
    pub fn update(&mut self, frametime: &Duration, mouse_pos: (f32, f32), mouse_down: bool) {
        let time_norm = frametime.as_micros() as f32 / 16666.0;
        let fric_norm = f32::powf(0.988, time_norm);
        let grav_norm = 1.0 * time_norm;

        let time_norm = F32s::splat(time_norm);
        let fric_norm = F32s::splat(fric_norm);
        let grav_norm = F32s::splat(grav_norm);

        let mouse_down = F32s::splat(mouse_down as u32 as f32);
        let mouse_x = F32s::splat(mouse_pos.0);
        let mouse_y = F32s::splat(mouse_pos.1);

        let particles_chunk_len = usize::max(
            self.particles.len() / self.threadpool.thread_count() as usize / 10,
            1,
        );

        let particles_chunks = self.particles.chunks_mut(particles_chunk_len);

        self.threadpool.scoped(|scope| {
            for particles_chunk in particles_chunks {
                scope.execute(move |_| {
                    for particle in particles_chunk {
                        particle.apply_grav(&mouse_x, &mouse_y, &mouse_down, &grav_norm);

                        particle.apply_fric(&fric_norm);

                        particle.x += particle.dx * time_norm;
                        particle.y += particle.dy * time_norm;
                    }
                });
            }
        });
    }
}

pub struct Particle {
    pub x: F32s,
    pub y: F32s,
    dx: F32s,
    dy: F32s,
}

impl Particle {
    pub fn new_random(width: u32, height: u32) -> Self {
        let mut rng = rand::thread_rng();
        let mut x = [0_f32; 64];
        rng.fill(&mut x);
        let x = F32s::from_slice(&x) * F32s::splat(width as f32);
        let mut y = [0_f32; 64];
        rng.fill(&mut y);
        let y = F32s::from_slice(&y) * F32s::splat(height as f32);

        Self {
            x,
            y,
            dx: F32s::splat(0.0),
            dy: F32s::splat(0.0),
        }
    }

    #[inline(always)]
    pub fn apply_grav(
        &mut self,
        mouse_x: &F32s,
        mouse_y: &F32s,
        mouse_down: &F32s,
        grav_norm: &F32s,
    ) {
        let dx = self.x - mouse_x;
        let dy = self.y - mouse_y;
        let dist_inv_sqr = f32x64::sqrt(dx * dx + dy * dy);

        self.dx -= mouse_down * grav_norm * dx / dist_inv_sqr;
        self.dy -= mouse_down * grav_norm * dy / dist_inv_sqr;
    }

    pub fn apply_fric(&mut self, fric_norm: &F32s) {
        self.dx *= fric_norm;
        self.dy *= fric_norm;
    }
}
