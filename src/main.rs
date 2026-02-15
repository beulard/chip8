extern crate sdl3;

mod font;

use core::panic;
use rand::RngExt;
use rand::rngs::ThreadRng;
use sdl3::audio::{AudioCallback, AudioFormat, AudioSpec, AudioStream};
use sdl3::keyboard::{Keycode, Scancode};
use sdl3::pixels::Color;
use sdl3::rect::Point;
use sdl3::render::{FRect, WindowCanvas};
use std::time::{Duration, Instant};

/// Display scale factor.
const SCALE_FACTOR: usize = 12;

/// Display scale factor.
const DISPLAY_WIDTH: usize = 64;
const DISPLAY_HEIGHT: usize = 32;

/// Target frame time.
/// For some reason, the quirks test will not register my display interrupt wait unless the frame rate is slightly lower than 60fps.
const FRAMETIME_US: u128 = 16800;

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
    pixels: [bool; DISPLAY_WIDTH * DISPLAY_HEIGHT],
}

impl Chip8Display {
    fn clear(&mut self) -> () {
        self.pixels.fill(false);
    }

    fn get_mut(&mut self, x: u8, y: u8) -> &mut bool {
        return self
            .pixels
            .get_mut(x as usize + (y as usize) * DISPLAY_WIDTH)
            .unwrap();
    }
}

#[derive(Debug)]
struct Chip8Keypad {
    pressed: [bool; 16],
    pressed_last: [bool; 16],
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
    v: [u8; 16],
    delay_timer: u8,
    sound_timer: u8,
    stack: Chip8Stack,
    display: Chip8Display,
    rng: ThreadRng,
    /// If true, stick to the cosmac quirks
    cosmac_quirks: bool,
    /// Used to update timers
    elapsed_us: u128,
}

impl Chip8State {
    fn new(rom: &[u8], cosmac: bool) -> Self {
        let mut ram: [u8; _] = [0; 4096];

        // Copy font into ram
        ram[0x50..=0x9F].copy_from_slice(&font::FONT);
        ram[0x200..0x200 + rom.len()].copy_from_slice(rom);

        Chip8State {
            ram: ram,
            pc: 0x200,
            i: 0,
            v: [0; 16],
            delay_timer: 0,
            sound_timer: 0,
            stack: Chip8Stack::new(),
            display: Chip8Display { pixels: [false; _] },
            rng: rand::rng(),
            cosmac_quirks: cosmac,
            elapsed_us: 0,
        }
    }

    fn update(&mut self, delta: Duration, keypad: &Chip8Keypad, blank_interrupt: bool) {
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

        let instr_bytes: [u8; 2] = self.ram[self.pc as usize..]
            .chunks(2)
            .next()
            .expect("Tried to fetch beyond end of ram")
            .try_into()
            .unwrap();

        // dbg!(instr_bytes);

        // Big endian
        let instr = u16::from_be_bytes(instr_bytes);

        // println!("0x{:04x}", instr);

        self.pc += 2;

        // Decode + execute

        let x = ((instr & 0x0f00) >> 8) as usize;
        let y = ((instr & 0x00f0) >> 4) as usize;
        let n = instr & 0x000f;
        let nn = (instr & 0x00ff) as u8;
        let nnn = instr & 0x0fff;

        match (instr & 0xf000) >> 12 {
            0x0 => {
                if instr == 0x00e0 {
                    // 0x00e0: clear display
                    self.display.clear();
                } else if instr == 0x00ee {
                    // 0x00ee: return from subroutine
                    self.pc = self.stack.pop();
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            0x1 => {
                // 0x1nnn: jump
                self.pc = nnn;
            }
            0x2 => {
                // 0x2nnn: call subroutine
                self.stack.push(self.pc);
                self.pc = nnn;
            }
            0x3 => {
                // 0x3xnn: skip if vx == nn
                if self.v[x] == nn {
                    self.pc += 2;
                }
            }
            0x4 => {
                // 0x4xnn: skip if vx != nn
                if self.v[x] != nn {
                    self.pc += 2;
                }
            }
            0x5 => {
                // 0x5xy0: skip if vx == vy
                if n == 0x0 {
                    if self.v[x] == self.v[y] {
                        self.pc += 2;
                    }
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            0x6 => {
                // 0x6xnn: load vx with immediate value
                self.v[x] = nn;
            }
            0x7 => {
                // 0x7xnn: add value to register vx
                self.v[x] = self.v[x].wrapping_add(nn);
            }
            0x8 => {
                // arithmetic
                if n == 0x0 {
                    // 0x8xy0: set
                    self.v[x] = self.v[y];
                } else if n == 0x1 {
                    // 0x8xy1: binary or
                    self.v[x] = self.v[x] | self.v[y];
                    if self.cosmac_quirks {
                        self.v[0xf] = 0;
                    }
                } else if n == 0x2 {
                    // 0x8xy2: binary and
                    self.v[x] = self.v[x] & self.v[y];
                    if self.cosmac_quirks {
                        self.v[0xf] = 0;
                    }
                } else if n == 0x3 {
                    // 0x8xy3: binary xor
                    self.v[x] = self.v[x] ^ self.v[y];
                    // dbg!(self.cosmac_quirks);
                    if self.cosmac_quirks {
                        self.v[0xf] = 0;
                    }
                } else if n == 0x4 {
                    // 0x8xy4: add
                    let (value, overflow) = self.v[x].overflowing_add(self.v[y]);
                    self.v[x] = value;
                    self.v[0xf] = if overflow { 1 } else { 0 };
                } else if n == 0x5 {
                    // 0x8xy5: subtract vx - vy
                    let (value, overflow) = self.v[x].overflowing_sub(self.v[y]);
                    self.v[x] = value;
                    self.v[0xf] = if overflow { 0 } else { 1 };
                } else if n == 0x6 {
                    // 0x8xy6: shift right
                    if self.cosmac_quirks {
                        self.v[x] = self.v[y];
                    }
                    let bit = self.v[x] & 0b1;
                    self.v[x] = self.v[x] >> 1;
                    self.v[0xf] = bit;
                } else if n == 0x7 {
                    // 0x8xy7: subtract vy - vx
                    let (value, overflow) = self.v[y].overflowing_sub(self.v[x]);
                    self.v[x] = value;
                    self.v[0xf] = if overflow { 0 } else { 1 };
                } else if n == 0xe {
                    // 0x8xye: shift left
                    if self.cosmac_quirks {
                        self.v[x] = self.v[y];
                    }
                    let bit = (self.v[x] & 0b10000000) >> 7;
                    self.v[x] = self.v[x] << 1;
                    self.v[0xf] = bit;
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            0x9 => {
                // 0x9xy0: skip if vx != vy
                if n == 0x0 {
                    if self.v[x] != self.v[y] {
                        self.pc += 2;
                    }
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            0xa => {
                // 0xannn: load index register with immediate value
                self.i = nnn;
            }
            0xb => {
                // 0xbnnn: jump to v0 + nnn
                self.pc = nnn + self.v[0x0] as u16;
            }
            0xc => {
                // 0xcxnn: rng
                self.v[x] = self.rng.random::<u8>() & nn;
            }
            0xd => {
                // 0xdxyn: draw sprite

                if !blank_interrupt {
                    // Block on this instruction until the next render
                    self.pc -= 2;
                } else {
                    self.v[0xf] = 0;

                    let sprite_addr = self.i;
                    let mut posy = self.v[y] % (DISPLAY_HEIGHT as u8);

                    'yloop: for row in 0..n {
                        let mut posx = self.v[x] % (DISPLAY_WIDTH as u8);
                        let data = self.ram[(sprite_addr + row) as usize];

                        'xloop: for bit_idx in (0..8).rev() {
                            let value = (data >> bit_idx) & 0b1;
                            let pixel = self.display.get_mut(posx, posy);

                            if value == 0b1 {
                                if *pixel {
                                    self.v[0xf] = 1;
                                }
                                *pixel = !*pixel;
                            }
                            posx += 1;
                            if posx as usize >= DISPLAY_WIDTH {
                                break 'xloop;
                            }
                        }

                        posy += 1;
                        if posy as usize >= DISPLAY_HEIGHT {
                            break 'yloop;
                        }
                    }
                }
            }
            0xe => {
                if nn == 0x9e {
                    // 0xex9e: skip if key in vx is pressed
                    if keypad.pressed[self.v[x] as usize] {
                        self.pc += 2;
                    }
                } else if nn == 0xa1 {
                    // 0xexa1: skip if key in vx is not pressed
                    if !keypad.pressed[self.v[x] as usize] {
                        self.pc += 2;
                    }
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            0xf => {
                if nn == 0x07 {
                    // 0xfx15: get delay timer
                    self.v[x] = self.delay_timer;
                } else if nn == 0x15 {
                    // 0xfx15: set delay timer
                    self.delay_timer = self.v[x];
                } else if nn == 0x18 {
                    // 0xfx18: set sound timer
                    self.sound_timer = self.v[x];
                } else if nn == 0x1e {
                    // 0xfx15: add to index
                    self.i += self.v[x] as u16;
                    if self.i >= 0x1000 {
                        self.v[0xf] = 1;
                        self.i = self.i % 0x1000;
                    }
                } else if nn == 0x0a {
                    // 0xfx0a: get key
                    let mut k: u8 = 16;
                    for i in 0..16 {
                        if keypad.pressed_last[i as usize] && !keypad.pressed[i as usize] {
                            k = i;
                            break;
                        }
                    }
                    if k > 15 {
                        // Keep executing this instruction until some key is pressed
                        self.pc -= 2;
                    } else {
                        self.v[x] = k;
                    }
                } else if nn == 0x29 {
                    // 0xfx29: set index to a font sprite
                    self.i = 0x50 + self.v[x] as u16 * 5;
                } else if nn == 0x33 {
                    // 0xfx33: vx to decimal
                    let mut vx = self.v[x];
                    self.ram[self.i as usize] = vx / 100;
                    vx = vx % 100;
                    self.ram[(self.i + 1) as usize] = vx / 10;
                    vx = vx % 10;
                    self.ram[(self.i + 2) as usize] = vx;
                    // println!(
                    //     "{} -> {} {} {}",
                    //     self.v[x],
                    //     self.ram[(self.i + 0) as usize],
                    //     self.ram[(self.i + 1) as usize],
                    //     self.ram[(self.i + 2) as usize]
                    // );
                } else if nn == 0x55 {
                    // 0xfx55: store to ram
                    // dbg!(self.i, self.v[x], self.ram[self.i as usize]);
                    if self.cosmac_quirks {
                        for i in 0..=x {
                            self.ram[self.i as usize] = self.v[i];
                            self.i += 1;
                        }
                    } else {
                        for i in 0..=x {
                            self.ram[self.i as usize + i] = self.v[i];
                        }
                    }
                } else if nn == 0x65 {
                    // 0xfx65: load from ram
                    if self.cosmac_quirks {
                        for i in 0..=x {
                            self.v[i] = self.ram[self.i as usize];
                            self.i += 1;
                        }
                    } else {
                        for i in 0..=x {
                            self.v[i] = self.ram[self.i as usize + i];
                        }
                    }
                } else {
                    panic!("Unknown instruction 0x{:02x}", instr);
                }
            }
            _ => {
                println!("Unknown instruction 0x{:02x}", instr);
            } //panic!("Invalid instruction 0x{:02x}", instr),
        }
    }
}

struct SquareWave {
    phase_inc: f32,
    phase: f32,
    volume: f32,
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
        Ok(value) => {
            if value == "" {
                false
            } else {
                true
            }
        }
        Err(_) => false,
    };
    dbg!(grid);
    let cosmac_quirks = match std::env::var("CHIP8_COSMAC_QUIRKS") {
        Ok(value) => {
            if value == "" {
                false
            } else {
                true
            }
        }
        Err(_) => false,
    };
    dbg!(cosmac_quirks);

    let sdl_context = sdl3::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let audio_subsystem = sdl_context.audio().unwrap();

    let source_freq = 44100;
    let source_spec = AudioSpec {
        freq: Some(source_freq),
        channels: Some(1),
        format: Some(AudioFormat::f32_sys()),
    };

    let dev = audio_subsystem
        .open_playback_stream(
            &source_spec,
            SquareWave {
                phase_inc: 440.0 / source_freq as f32,
                phase: 0.0,
                volume: 0.05,
            },
        )
        .unwrap();
    let mut beeping = false;

    let window = video_subsystem
        .window(
            "chip8 interpreter",
            (DISPLAY_WIDTH * SCALE_FACTOR) as u32,
            (DISPLAY_HEIGHT * SCALE_FACTOR) as u32,
        )
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

    // Load rom into ram
    let mut args = std::env::args();
    args.next();

    let rom_path: String = match args.next() {
        Some(path) => path,
        None => panic!("No rom path provided"),
    };
    let rom_data = std::fs::read(rom_path).unwrap();

    let num_cycles: usize = match args.next() {
        Some(cycles) => cycles.parse().unwrap(),
        None => 0,
    };

    let mut chip8_state = Chip8State::new(&rom_data, cosmac_quirks);

    let mut cycle_idx = 0;

    let mut keypad = Chip8Keypad {
        pressed: [false; 16],
        pressed_last: [false; 16],
    };

    let mut just_rendered = false;

    'running: loop {
        // Handle events
        for event in event_pump.poll_iter() {
            use sdl3::event::Event;
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        // Update in as many fixed steps
        lag_us += prev_update.elapsed().as_micros();
        // Number of cycles to simulate.
        while lag_us >= CHIP8_UPDATE_TIME_US {
            // println!("lag_us={} us_per_update={}", lag_us, CHIP8_UPDATE_TIME_US);
            let delta = prev_update.elapsed();
            prev_update = Instant::now();

            let kb = event_pump.keyboard_state();

            keypad.pressed_last = keypad.pressed;
            keypad.pressed = [
                kb.is_scancode_pressed(Scancode::X),
                kb.is_scancode_pressed(Scancode::_1),
                kb.is_scancode_pressed(Scancode::_2),
                kb.is_scancode_pressed(Scancode::_3),
                kb.is_scancode_pressed(Scancode::Q),
                kb.is_scancode_pressed(Scancode::W),
                kb.is_scancode_pressed(Scancode::E),
                kb.is_scancode_pressed(Scancode::A),
                kb.is_scancode_pressed(Scancode::S),
                kb.is_scancode_pressed(Scancode::D),
                kb.is_scancode_pressed(Scancode::Z),
                kb.is_scancode_pressed(Scancode::C),
                kb.is_scancode_pressed(Scancode::_4),
                kb.is_scancode_pressed(Scancode::R),
                kb.is_scancode_pressed(Scancode::F),
                kb.is_scancode_pressed(Scancode::V),
            ];

            if cycle_idx < num_cycles || num_cycles == 0 {
                chip8_state.update(delta, &keypad, just_rendered);
                just_rendered = false;
                cycle_idx += 1;
                if cycle_idx == num_cycles {
                    println!("Stopping interpreter after {} cycles", num_cycles);
                }
                if chip8_state.sound_timer > 0 && !beeping {
                    beeping = true;
                    dev.resume().unwrap();
                } else if beeping && chip8_state.sound_timer == 0 {
                    beeping = false;
                    dev.pause().unwrap();
                }
            }

            // println!("update time: {} us", update_start.elapsed().as_micros());
            lag_us -= CHIP8_UPDATE_TIME_US;
        }

        if prev_render.elapsed().as_micros() > FRAMETIME_US {
            let framerate = 1.0 / prev_render.elapsed().as_secs_f64();
            prev_render = Instant::now();
            render(&mut canvas, &chip8_state.display, framerate, grid);
            just_rendered = true;
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
            rects.push(FRect::new(
                x as f32,
                y as f32,
                SCALE_FACTOR as f32,
                SCALE_FACTOR as f32,
            ));
        }
    }
    canvas.fill_rects(&rects).expect("?");

    if grid {
        canvas.set_draw_color(Color::RGB(50, 50, 50));
        for i in 0..DISPLAY_WIDTH {
            canvas
                .draw_line(
                    ((i * SCALE_FACTOR) as f32 - 1.0, 0.0),
                    (
                        (i * SCALE_FACTOR) as f32 - 1.0,
                        (DISPLAY_HEIGHT * SCALE_FACTOR) as f32,
                    ),
                )
                .unwrap();
        }

        for i in 0..DISPLAY_HEIGHT {
            canvas
                .draw_line(
                    (0.0, (i * SCALE_FACTOR) as f32 - 1.0),
                    (
                        (DISPLAY_WIDTH * SCALE_FACTOR) as f32,
                        (i * SCALE_FACTOR) as f32 - 1.0,
                    ),
                )
                .unwrap();
        }
    }

    canvas.set_draw_color(Color::RGB(165, 165, 165));
    canvas
        .draw_debug_text(&format!("{:.1}", framerate), Point::new(5, 5))
        .unwrap();
    canvas.present();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_0() {
        // let input = 254;
        // let input = 33;
        let input = 9;

        let mut vx = input;

        let d0 = vx / 100;
        vx = vx % 100;
        let d1 = vx / 10;
        vx = vx % 10;
        let d2 = vx;
        println!("{} -> {} {} {}", input, d0, d1, d2,);
    }
}
