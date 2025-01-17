use core::{f32, panic};
use std::cell::RefCell;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use scoped_threadpool::Pool;
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::particles::Particles;

const THREADS: usize = 16;

struct AppData {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    data: Option<AppData>,
    last_frame_time: Instant,
    n_frame: u32,
    particles: Particles,
    threadpool: Rc<RefCell<Pool>>,
    mouse_pos: Option<(f32, f32)>,
    mouse_down: bool,
}

impl Default for App {
    fn default() -> Self {
        let threadpool = Rc::new(RefCell::new(Pool::new(THREADS as u32)));
        let particles = Particles::new(5_000_000, 1280, 720, Rc::clone(&threadpool));
        App {
            data: None,
            last_frame_time: Instant::now(),
            n_frame: 0,
            threadpool,
            particles,
            mouse_pos: None,
            mouse_down: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Rc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        let context = Context::new(Rc::clone(&window)).unwrap();
        let surface = softbuffer::Surface::new(&context, Rc::clone(&window)).unwrap();
        self.data = Some(AppData { surface, window })
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let _ = id;
        let Some(AppData { window, surface }) = &mut self.data else {
            panic!();
        };
        event_loop.set_control_flow(ControlFlow::Poll);

        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::CursorMoved {
                device_id: _,
                position,
            } => {
                self.mouse_pos = Some((position.x as f32, position.y as f32));
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button: MouseButton::Left,
            } => {
                self.mouse_down = state == ElementState::Pressed;
            }
            WindowEvent::RedrawRequested => {
                self.n_frame += 1;
                let now = Instant::now();
                let frametime = now.duration_since(self.last_frame_time);
                self.last_frame_time = now;
                if self.n_frame % 100 == 0 {
                    println!("#{}: FPS = {}", self.n_frame, 1.0 / frametime.as_secs_f32());
                }

                let (width, height) = {
                    let size = window.inner_size();
                    (size.width as usize, size.height as usize)
                };

                surface
                    .resize(
                        NonZeroU32::new(width as u32).unwrap(),
                        NonZeroU32::new(height as u32).unwrap(),
                    )
                    .unwrap();

                self.particles
                    .update(&frametime, self.mouse_pos, self.mouse_down);

                let count_chunk_len = self.particles.particles.len() / THREADS;

                let particles_chunks = self.particles.particles.chunks(count_chunk_len);

                let mut super_count_buffer = (0..particles_chunks.len())
                    .map(|_| vec![0_u16; width * height])
                    .collect::<Vec<_>>();

                // let mut count_buffer = vec![0_u16; width * height * COUNT_CHUNK_LEN];
                // let super_count_buffer = &count_buffer
                //     .chunks_exact_mut(COUNT_CHUNK_LEN)
                //     .collect::<Vec<_>>();

                self.threadpool.borrow_mut().scoped(|scope| {
                    for (particles_chunk, count_buffer) in
                        particles_chunks.zip(super_count_buffer.iter_mut())
                    {
                        scope.execute(move || {
                            for particle in particles_chunk {
                                if particle.x < 0.0
                                    || particle.x as usize >= width - 1
                                    || particle.y < 0.0
                                    || particle.y as usize >= height - 1
                                {
                                    continue;
                                }

                                count_buffer[particle.x as usize + particle.y as usize * width] +=
                                    1;
                            }
                        });
                    }
                });

                let mut pixel_buffer = surface.buffer_mut().unwrap();
                pixel_buffer.iter_mut().for_each(|pixel| *pixel = 0);

                let pixel_chunk_len = width * height / THREADS / 2;

                let pixel_buffer_chunks = pixel_buffer
                    .chunks_exact_mut(pixel_chunk_len)
                    .collect::<Vec<_>>();

                let super_count_buffer_chunks = &super_count_buffer
                    .iter()
                    .map(|count_buffer| {
                        count_buffer
                            .chunks_exact(pixel_chunk_len)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();

                self.threadpool.borrow_mut().scoped(|scope| {
                    for (i_chunk, pixel_buffer_chunk) in pixel_buffer_chunks.into_iter().enumerate()
                    {
                        let mut count_chunks = Vec::new();
                        for count_buffer_chunks in super_count_buffer_chunks {
                            count_chunks.push(count_buffer_chunks[i_chunk]);
                        }
                        scope.execute(move || {
                            for (i_pixel, pixel) in pixel_buffer_chunk.iter_mut().enumerate() {
                                let index = i_chunk * pixel_chunk_len + i_pixel;
                                let x = (index % width) as f32;
                                let y = (index / width) as f32;
                                let count = count_chunks.iter().fold(0_u16, |a, b| a + b[i_pixel])
                                    as f32
                                    * 32.0; // for pixel in pixel_buffer_chunk.iter_mut() {
                                let red = (x * count / width as f32) as u8;
                                let green = (y * count / height as f32) as u8;
                                let blue = ((1.0 - (x / width as f32) - (y / height as f32))
                                    * count) as u8;
                                *pixel = ((red as u32) << THREADS)
                                    + ((green as u32) << 8)
                                    + (blue as u32)
                            }
                        });
                    }
                });

                // let x = particle.x as usize;
                // let y = particle.y as usize;
                // let red = particle.x * 255.0 / width as f32;
                // let green = particle.y * 255.0 / height as f32;
                // let blue: f32 =
                //     (1.0 - (particle.x / width as f32) - (particle.y / height as f32)) * 255.0;

                // let index = x + y * width as usize;

                // let val = &mut buffer[index];
                // let mut colors = val.to_le_bytes();
                // if *val == 0 {
                //     *val = ((red as u32) << THREADS) + ((green as u32) << 8) + (blue as u32);
                // } else {
                //     colors.iter_mut().take(3).for_each(|c| {
                //         *c = (*c).saturating_add(2);
                //     });
                //     *val = u32::from_le_bytes(colors);
                // }

                // for particle in &self.particles.particles {
                //     if particle.x < 0.0
                //         || particle.x >= width as f32
                //         || particle.y < 0.0
                //         || particle.y >= height as f32
                //     {
                //         continue;
                //     }
                //     let x = particle.x as usize;
                //     let y = particle.y as usize;
                //     let red = particle.x * 255.0 / width as f32;
                //     let green = particle.y * 255.0 / height as f32;
                //     let blue: f32 =
                //         (1.0 - (particle.x / width as f32) - (particle.y / height as f32)) * 255.0;
                //     buffer[x + y * width as usize] =
                //         ((red as u32) << THREADS) + ((green as u32) << 8) + (blue as u32);
                // }

                pixel_buffer.present().unwrap();
                window.request_redraw();
            }
            _ => (),
        }
    }
}

pub fn run() {
    let event_loop = EventLoop::new().unwrap();

    // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
    // dispatched any events. This is ideal for games and similar applications.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    let _ = event_loop.run_app(&mut app);
}
