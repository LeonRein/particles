use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::SystemTime;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::particles::Particles;

struct App {
    window: Option<Rc<Window>>,
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    last_frame_time: SystemTime,
    n_frame: u32,
    particles: Particles,
}

impl Default for App {
    fn default() -> Self {
        App {
            window: None,
            surface: None,
            context: None,
            last_frame_time: SystemTime::now(),
            n_frame: 0,
            particles: Particles::new(100_000, 1280, 720),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(Rc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        ));
        if let Some(window) = &self.window {
            let context = Context::new(Rc::clone(window)).unwrap();
            self.surface = Some(softbuffer::Surface::new(&context, Rc::clone(window)).unwrap());
            self.context = Some(context);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let _ = id;
        let Some(window) = &self.window else {
            return;
        };
        let Some(surface) = &mut self.surface else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.n_frame += 1;
                let now = SystemTime::now();
                let frametime = now.duration_since(self.last_frame_time).unwrap_or_default();
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
                    .update(&frametime, Some((1280.0 / 2.0, 720.0 / 2.0)), false);

                let mut buffer = surface.buffer_mut().unwrap();
                // for index in 0..(width * height) {
                //     let y = index / width;
                //     let x = index % width;
                //     let red = x % 255;
                //     let green = y % 255;
                //     let blue = (x * y) % 255;

                //     buffer[index as usize] = blue | (green << 8) | (red << 16);
                // }

                for particle in &self.particles.particles {
                    if particle.x < 0.0
                        || particle.x >= width as f32
                        || particle.y < 0.0
                        || particle.y >= height as f32
                    {
                        continue;
                    }
                    let x = particle.x as usize;
                    let y = particle.y as usize;
                    let red = particle.x * 255.0 / width as f32;
                    let green = particle.y * 255.0 / height as f32;
                    let blue: f32 =
                        (1.0 - (particle.x / width as f32) - (particle.y / height as f32)) * 255.0;
                    buffer[x + y * width as usize] =
                        ((red as u32) << 16) + ((green as u32) << 8) + (blue as u32);
                }

                buffer.present().unwrap();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
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
