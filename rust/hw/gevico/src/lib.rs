// SPDX-License-Identifier: GPL-2.0-or-later

mod bindings;

use core::ffi::c_void;
use std::ffi::c_char;

use hwcore::bindings::DeviceState;

use bindings::{at24c_eeprom_init, i2c_end_transfer, i2c_init_bus, i2c_recv, i2c_send, i2c_start_transfer, I2CBus};

const REG_CTRL: u64 = 0x00;
const REG_STATUS: u64 = 0x04;
const REG_ADDR: u64 = 0x08;
const REG_DATA: u64 = 0x0c;
const REG_PRESCALE: u64 = 0x10;

const CTRL_EN: u32 = 1 << 0;
const CTRL_START: u32 = 1 << 1;
const CTRL_STOP: u32 = 1 << 2;
const CTRL_RW: u32 = 1 << 3;

const ST_BUSY: u32 = 1 << 0;
const ST_ACK: u32 = 1 << 1;
const ST_DONE: u32 = 1 << 2;

const AT24C02_ADDR: u8 = 0x50;
const AT24C02_SIZE: u32 = 256;

const BUS_NAME: &[u8] = b"i2c-gpio-bus\0";

#[repr(C)]
struct I2CGpioRust {
    ctrl: u32,
    status: u32,
    addr: u8,
    data: u8,
    prescale: u32,
    started: bool,
    bus: *mut I2CBus,
}

impl I2CGpioRust {
    fn set_done_ack(&mut self, ack: bool) {
        self.status &= !(ST_ACK | ST_DONE);
        if ack {
            self.status |= ST_ACK;
        }
        self.status |= ST_DONE;
    }

    fn do_ctrl(&mut self, val: u32) {
        self.ctrl = val;

        if (val & CTRL_EN) == 0 {
            self.status &= !(ST_BUSY | ST_ACK);
            self.status |= ST_DONE;
            return;
        }

        if (val & CTRL_START) != 0 {
            let is_recv = (val & CTRL_RW) != 0;
            let rc = unsafe { i2c_start_transfer(self.bus, self.addr, is_recv) };

            self.started = rc == 0;
            if self.started {
                self.status |= ST_BUSY;
            } else {
                self.status &= !ST_BUSY;
            }
            self.set_done_ack(rc == 0);
            return;
        }

        if (val & CTRL_STOP) != 0 {
            if self.started {
                unsafe {
                    i2c_end_transfer(self.bus);
                }
            }
            self.started = false;
            self.status &= !(ST_BUSY | ST_ACK);
            self.status |= ST_DONE;
            return;
        }

        if !self.started {
            self.set_done_ack(false);
            return;
        }

        if (val & CTRL_RW) != 0 {
            self.data = unsafe { i2c_recv(self.bus) };
            self.set_done_ack(true);
        } else {
            let rc = unsafe { i2c_send(self.bus, self.data) };
            self.set_done_ack(rc == 0);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn i2c_gpio_rust_new(parent: *mut DeviceState) -> *mut c_void {
    let bus = unsafe { i2c_init_bus(parent, BUS_NAME.as_ptr() as *const c_char) };
    if bus.is_null() {
        return core::ptr::null_mut();
    }

    unsafe {
        at24c_eeprom_init(bus, AT24C02_ADDR, AT24C02_SIZE);
    }

    let state = Box::new(I2CGpioRust {
        ctrl: 0,
        status: 0,
        addr: 0,
        data: 0,
        prescale: 0,
        started: false,
        bus,
    });

    Box::into_raw(state) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn i2c_gpio_rust_free(state: *mut c_void) {
    if state.is_null() {
        return;
    }
    let mut boxed = unsafe { Box::from_raw(state as *mut I2CGpioRust) };
    if boxed.started {
        unsafe {
            i2c_end_transfer(boxed.bus);
        }
    }
    boxed.started = false;
}

#[no_mangle]
pub unsafe extern "C" fn i2c_gpio_rust_reset(state: *mut c_void) {
    if state.is_null() {
        return;
    }
    let s = unsafe { &mut *(state as *mut I2CGpioRust) };
    if s.started {
        unsafe {
            i2c_end_transfer(s.bus);
        }
    }
    s.ctrl = 0;
    s.status = 0;
    s.addr = 0;
    s.data = 0;
    s.prescale = 0;
    s.started = false;
}

#[no_mangle]
pub unsafe extern "C" fn i2c_gpio_rust_read(state: *mut c_void, offset: u64) -> u32 {
    if state.is_null() {
        return 0;
    }
    let s = unsafe { &mut *(state as *mut I2CGpioRust) };
    match offset {
        REG_CTRL => s.ctrl,
        REG_STATUS => s.status,
        REG_ADDR => u32::from(s.addr),
        REG_DATA => u32::from(s.data),
        REG_PRESCALE => s.prescale,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn i2c_gpio_rust_write(state: *mut c_void, offset: u64, value: u32) {
    if state.is_null() {
        return;
    }
    let s = unsafe { &mut *(state as *mut I2CGpioRust) };
    match offset {
        REG_CTRL => s.do_ctrl(value),
        REG_ADDR => s.addr = value as u8,
        REG_DATA => s.data = value as u8,
        REG_PRESCALE => s.prescale = value,
        _ => {}
    }
}