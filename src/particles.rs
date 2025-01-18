use std::time::Duration;

use crate::scoped_threadpool::Pool;
use rand::Rng;

pub struct Particles<'a> {
    pub particles: Vec<Particle>,
    threadpool: &'a Pool,
}

impl<'a> Particles<'a> {
    pub fn new(n: usize, width: usize, height: usize, threadpool: &'a Pool) -> Self {
        let mut particles = Vec::<Particle>::with_capacity(n);
        for _ in 0..n {
            particles.push(Particle::new_random(width, height));
        }
        Self {
            particles,
            threadpool,
        }
    }

    pub fn add_particles(&mut self, n: usize, width: usize, height: usize) {
        for _ in 0..n {
            self.particles.push(Particle::new_random(width, height));
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
                        if mouse_down {
                            if let Some((x, y)) = mouse_pos {
                                particle.apply_grav(x, y, grav_norm);
                            }
                        }

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
