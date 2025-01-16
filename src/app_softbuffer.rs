use core::f32;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;

use softbuffer::{Context, Surface};
use threadpool::ThreadPool;
use threadpool_scope::scope_with;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::particles::Particles;

struct AppData {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    data: Option<AppData>,
    last_frame_time: Instant,
    n_frame: u32,
    particles: Particles,
    thread_pool: ThreadPool,
    mouse_pos: Option<(f32, f32)>,
    mouse_down: bool,
}

impl Default for App {
    fn default() -> Self {
        App {
            data: None,
            last_frame_time: Instant::now(),
            n_frame: 0,
            particles: Particles::new(10_000_000, 1280, 720),
            thread_pool: ThreadPool::new(10),
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

        self.n_frame += 1;
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
                let now = Instant::now();
                let frametime = now.duration_since(self.last_frame_time);
                self.last_frame_time = now;
                if self.n_frame % 100 == 0 {
                    println!("#{}: FPS = {}", self.n_frame, 1.0 / frametime.as_secs_f32());
                }

                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };

                surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                self.particles
                    .update(&frametime, self.mouse_pos, self.mouse_down);

                let mut buffer = surface.buffer_mut().unwrap();
                let n_buffer_chunks = 10000;
                let buffer_chunk_size = buffer.len() / n_buffer_chunks;

                for pixel in buffer.iter_mut() {
                    *pixel = 0;
                }

                let buffer_chunks = &buffer
                    .chunks_exact_mut(buffer_chunk_size)
                    .map(Mutex::new)
                    .collect::<Vec<_>>();
                let particles_chunks = self.particles.particles.chunks(10000);

                let buffer_chunk_size = buffer_chunks[0].lock().unwrap().len();

                scope_with(&self.thread_pool, |scope| {
                    for particles_chunk in particles_chunks {
                        scope.execute(move || {
                            for particle in particles_chunk {
                                if particle.x < 0.0
                                    || particle.x as usize >= width as usize
                                    || particle.y < 0.0
                                    || particle.y as usize >= height as usize
                                {
                                    continue;
                                }

                                let x = particle.x.floor() as usize;
                                let y = particle.y.floor() as usize;
                                let red = particle.x * 255.0 / width as f32;
                                let green = particle.y * 255.0 / height as f32;
                                let blue: f32 = (1.0
                                    - (particle.x / width as f32)
                                    - (particle.y / height as f32))
                                    * 255.0;
                                let index = x + y * width as usize;
                                let bufffer_chunk_index = index / buffer_chunk_size;
                                if bufffer_chunk_index > buffer_chunks.len() - 1 {
                                    return;
                                }
                                let Ok(mut buffer_chunk) =
                                    buffer_chunks[index / buffer_chunk_size].lock()
                                else {
                                    // println!("could not get lock");
                                    continue;
                                };

                                buffer_chunk[index % buffer_chunk_size] =
                                    ((red as u32) << 16) + ((green as u32) << 8) + (blue as u32);
                            }
                        });
                    }
                });
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
                //         ((red as u32) << 16) + ((green as u32) << 8) + (blue as u32);
                // }

                buffer.present().unwrap();
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
