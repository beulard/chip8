extern crate sdl3;

mod font;

use core::panic;
use std::ops::Div;
use std::usize;
use sdl3::audio::{self, AudioCallback, AudioFormat, AudioSpec, AudioStream};
use sdl3::keyboard::{Keycode, Scancode};
use sdl3::pixels::Color;
use sdl3::rect::{Point, Rect};
use sdl3::render::FRect;
use sdl3::{event::Event, render::WindowCanvas};
use std::time::{Duration, Instant};

/// Display scale factor.
const SCALE_FACTOR: usize = 16;

/// Display scale factor.
const DISPLAY_WIDTH: usize = 64;
const DISPLAY_HEIGHT: usize = 32;

/// Target frame time.
const FRAMETIME_US: u128 = 16667;

const TIMER_DECREMENT_INTERVAL_US: u128 = 16667;

/// Number of microseconds between two chip8 clock cycles.
const CHIP8_UPDATE_TIME_US: u128 = 1429; // 1429 = 1000000 / 700 (700Hz)

/// LIFO stack
const STACK_CAPACITY: usize = 64;

#[derive(Debug)]
#[allow(unused)]
struct Chip8Stack {
    buffer: [u16; STACK_CAPACITY],
    top: usize,
}

#[allow(unused)]
impl Chip8Stack {
    fn new() -> Self {
        return Chip8Stack {
            buffer: [0; _],
            top: 0,
        };
    }
    fn push(&mut self, value: u16) {
        if self.top == STACK_CAPACITY {
            panic!();
        }
        self.buffer[self.top] = value;
        self.top += 1;
    }
    fn pop(&mut self) -> u16 {
        if self.top == 0 {
            panic!();
        }
        self.top -= 1;
        return self.buffer[self.top];
    }
}

#[derive(Debug)]
struct Chip8Display {
    pixels: [bool; DISPLAY_WIDTH * DISPLAY_HEIGHT]
}

#[derive(Debug)]
struct Chip8Keypad {
    pressed: [bool; 16]
}

impl Chip8Keypad {
    fn new() -> Self {
         Chip8Keypad { pressed: [false; 16]}
    }
}


#[allow(unused)]
#[derive(Debug)]
struct Chip8State {
    ram: [u8; 4096],
    /// Program counter.
    pc: u16,
    /// Index register.
    i: u16,
    /// General purpose registers.
    v0: u8,
    v1: u8,
    v2: u8,
    v3: u8,
    v4: u8,
    v5: u8,
    v6: u8,
    v7: u8,
    v8: u8,
    v9: u8,
    va: u8,
    vb: u8,
    vc: u8,
    vd: u8,
    ve: u8,
    vf: u8,
    delay_timer: u8,
    sound_timer: u8,
    stack: Chip8Stack,
    display: Chip8Display,
    keypad: Chip8Keypad,

    //
    elapsed_us: u128,
}


impl Chip8State {
    fn new() -> Self {
        let mut ram: [u8; _] = [0; 4096];

        // Copy font into ram
        ram[0x50..=0x9F].copy_from_slice(&font::FONT);

        Chip8State {
            ram: ram,
            pc: 0,
            i: 0,
            v0: 0,
            v1: 0,
            v2: 0,
            v3: 0,
            v4: 0,
            v5: 0,
            v6: 0,
            v7: 0,
            v8: 0,
            v9: 0,
            va: 0,
            vb: 0,
            vc: 0,
            vd: 0,
            ve: 0,
            vf: 0,
            delay_timer: 0,
            sound_timer: 0,
            stack: Chip8Stack::new(),
            display: Chip8Display { pixels: [false; _]},
            keypad: Chip8Keypad::new(),
            elapsed_us: 0,
        }
    }
    fn update(&mut self, delta: Duration) {
        // Update timers
        self.elapsed_us += delta.as_micros();
        while self.elapsed_us >= TIMER_DECREMENT_INTERVAL_US {
            // println!("decrement timers");
            if self.delay_timer > 0 {
                self.delay_timer -= 1;
            }
            if self.sound_timer > 0 {
                self.sound_timer -= 1;
            }
            self.elapsed_us -= TIMER_DECREMENT_INTERVAL_US;
        }

        // Fetch
        // let instr = &self.ram[(self.pc as usize)..=(self.pc  as usize+ 1)];
        let i = self.pc as usize;
        let j = (self.pc+1) as usize;
        let instr = &self.ram[i..=j];
    }
}


struct SquareWave {
    phase_inc: f32,
    phase: f32,
    volume: f32
}

impl AudioCallback<f32> for SquareWave {
    fn callback(&mut self, stream: &mut AudioStream, requested: i32) {
        let mut out = Vec::<f32>::with_capacity(requested as usize);
        // Generate a square wave
        for _ in 0..requested {
            out.push(if self.phase <= 0.5 {
                self.volume
            } else {
                -self.volume
            });
            self.phase = (self.phase + self.phase_inc) % 1.0;
        }
        stream.put_data_f32(&out).expect("no bueno");
    }
}

pub fn main() {
    let grid = match std::env::var("CHIP8_GRID") {
        Ok(value) => match value.as_str() {
            "OFF" | "" => false,
            _ => true,
        },
        Err(_) => false,
    };


    let sdl_context = sdl3::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let audio_subsystem = sdl_context.audio().unwrap();

    let source_freq = 44100;
    let source_spec = AudioSpec {
        freq: Some(source_freq),
        channels: Some(1),
        format: Some(AudioFormat::f32_sys()),
    };

    let dev = audio_subsystem.open_playback_stream(&source_spec, SquareWave {
        phase_inc: 440.0 / source_freq as f32,
        phase: 0.0,
        volume: 0.15
    }).unwrap();

    dev.resume().unwrap();
    dev.pause().unwrap();



    let window = video_subsystem
        .window("chip8 interpreter", (DISPLAY_WIDTH  * SCALE_FACTOR) as u32, (DISPLAY_HEIGHT * SCALE_FACTOR) as u32)
        .position_centered()
        .borderless()
        .build()
        .expect("no bueno");

    let mut canvas = window.into_canvas();

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let mut event_pump = sdl_context.event_pump().unwrap();

    let mut prev_update = Instant::now();
    let mut lag_us = 0;
    let mut prev_render = Instant::now();

    let mut chip8_state = Chip8State::new();

    // chip8_state.display.pixels[4] = true;
    let mut i =0;

    'running: loop {
        // Handle events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                Event::KeyDown {scancode: Some(Scancode::_1), ..} => {},
                _ => {}
            }
        }

        // TODO how to update chip8 keypad given events ?
        // TODO implement instructions !

        // Update in as many fixed steps
        lag_us += prev_update.elapsed().as_micros();
        // Number of cycles to simulate.
        while lag_us >= CHIP8_UPDATE_TIME_US {
            // println!("lag_us={} us_per_update={}", lag_us, CHIP8_UPDATE_TIME_US);
            let delta = prev_update.elapsed();
            prev_update = Instant::now();

            // let update_start = Instant::now();
            chip8_state.update(delta);

            // println!("update time: {} us", update_start.elapsed().as_micros());
            lag_us -= CHIP8_UPDATE_TIME_US;
        }

        if prev_render.elapsed().as_micros() > FRAMETIME_US {
            let framerate = 1.0 / prev_render.elapsed().as_secs_f64();
            prev_render = Instant::now();
            render(&mut canvas, &chip8_state.display, framerate, grid);
            chip8_state.display.pixels[i] = !chip8_state.display.pixels[i];
            i = (i + 1) % (DISPLAY_WIDTH * DISPLAY_HEIGHT);
            // let render_time = prev_render.elapsed();
            // println!("render time: {} us", render_time.as_micros());
        }
    }
}

fn render(canvas: &mut WindowCanvas, display: &Chip8Display, framerate: f64, grid: bool) {
    canvas.set_draw_color(Color::RGB(10, 10, 10));
    canvas.clear();


    // Draw each pixel as a separate square of SCALE_FACTOR x SCALE_FACTOR
    let mut rects = vec![];
    canvas.set_draw_color(Color::RGB(255, 255, 190));
    for (i, pixel) in display.pixels.iter().enumerate() {
        if *pixel {
            let x = i % DISPLAY_WIDTH * SCALE_FACTOR;
            let y = i / DISPLAY_WIDTH * SCALE_FACTOR;
            rects.push(FRect::new(x as f32, y as f32, SCALE_FACTOR as f32, SCALE_FACTOR as f32));
            // canvas.fill_rect(Rect::new(x as i32, y as i32, SCALE_FACTOR as u32, SCALE_FACTOR as u32)).expect("q?");
        }
    }
    canvas.fill_rects(&rects).expect("?");

    if grid {
        canvas.set_draw_color(Color::RGB(50, 50, 50));
        for i in 0..DISPLAY_WIDTH {
            canvas.draw_line(((i * SCALE_FACTOR) as f32 - 1.0, 0.0), ((i * SCALE_FACTOR) as f32 - 1.0, (DISPLAY_HEIGHT * SCALE_FACTOR) as f32)).unwrap();
        }

        for i in 0..DISPLAY_HEIGHT {
            canvas.draw_line((0.0, (i * SCALE_FACTOR) as f32 - 1.0), ((DISPLAY_WIDTH * SCALE_FACTOR) as f32, (i * SCALE_FACTOR) as f32 - 1.0)).unwrap();
        }

    }

    canvas.set_draw_color(Color::RGB(165, 165, 165));
    canvas
        .draw_debug_text(&format!("{:.1}", framerate), Point::new(5, 5))
        .unwrap();
    canvas.present();
}
