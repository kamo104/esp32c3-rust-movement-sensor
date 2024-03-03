#![no_std]
#![no_main]

use core::time::Duration;

use esp32c3_hal::{
    clock::ClockControl, 
    peripherals::Peripherals, 
    prelude::*, 
    Delay,
    IO,
    Rtc,
    macros::ram,
    reset::SleepSource,
    gpio::RTCPinWithResistors,
    rtc_cntl::{
        get_wakeup_cause,
        sleep::{
            TimerWakeupSource,
            RtcioWakeupSource, 
            WakeupLevel
        },
    },
    systimer::SystemTimer, 
    Rng,
    efuse::Efuse,
};

use esp_wifi::{
    initialize, 
    EspWifiInitFor,
    esp_now::{
        BROADCAST_ADDRESS,
        PeerInfo,
        EspNow
    }
};


use esp_backtrace as _;
use esp_println::println;

#[ram(rtc_fast, uninitialized)]
static mut WAKEUP_LEVEL:WakeupLevel = WakeupLevel::High;
#[ram(rtc_fast, uninitialized)]
static mut SERVER_ADDR: [u8; 6] = BROADCAST_ADDRESS;
#[ram(rtc_fast, uninitialized)]
static mut TIMER_SLEEP: bool = false;

macro_rules! read_volatile {
    ($var:expr) => {
        unsafe{core::ptr::read_volatile(core::ptr::addr_of!($var))}
    };
}

macro_rules! write_volatile {
    ($var:expr, $val:expr) => {
        unsafe{core::ptr::write_volatile(core::ptr::addr_of_mut!($var), $val)}
    };
}

macro_rules! negate_wakeup_level {
    () => {
        if read_volatile!(WAKEUP_LEVEL) == WakeupLevel::High {
            write_volatile!(WAKEUP_LEVEL,WakeupLevel::Low);
        }
        else {
            write_volatile!(WAKEUP_LEVEL,WakeupLevel::High);
        }
    };
}


macro_rules! begin_sleep {
    ($rtc:ident, $rtcio:ident, $timer_wakeup:ident, $delay:ident) => {
        if read_volatile!(TIMER_SLEEP) {
            $rtc.sleep_deep(&[&$rtcio,&$timer_wakeup], &mut $delay);
        }
        $rtc.sleep_deep(&[&$rtcio], &mut $delay);
    };
}


#[entry]
fn main() -> ! {

    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::max(system.clock_control).freeze();
    let mut delay = Delay::new(&clocks);
    let mut rtc = Rtc::new(peripherals.LPWR);

    let wake_reason = get_wakeup_cause();
    println!("wake reason: {:?}", wake_reason);

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut wakeup_pin = io.pins.gpio2;
    wakeup_pin.set_to_input();
    let wakeup_pin_state = wakeup_pin.is_input_high();

    let status_led_pin = io.pins.gpio8;
    let mut status_led = status_led_pin.into_push_pull_output();
    status_led.set_high().unwrap();

    let mut send_update = false;
    match wake_reason{
        SleepSource::Timer =>{
            if read_volatile!(WAKEUP_LEVEL) == WakeupLevel::High {
                write_volatile!(TIMER_SLEEP,false);
                send_update = true;
            }
        }
        SleepSource::Gpio=>{
            if read_volatile!(WAKEUP_LEVEL) == WakeupLevel::High && read_volatile!(TIMER_SLEEP) == false {
                send_update = true;
            }
            else if read_volatile!(WAKEUP_LEVEL) == WakeupLevel::High && read_volatile!(TIMER_SLEEP) == true {
                write_volatile!(TIMER_SLEEP,false);
            }
            else if read_volatile!(WAKEUP_LEVEL) == WakeupLevel::Low {
                write_volatile!(TIMER_SLEEP,true);
            }
            negate_wakeup_level!();
        }
        SleepSource::Undefined=> write_volatile!(SERVER_ADDR,BROADCAST_ADDRESS),
        _ => (),
    }

    let timer_wakeup = TimerWakeupSource::new(Duration::from_secs(5));
    let wakeup_pins: &mut [(&mut dyn RTCPinWithResistors, WakeupLevel)] = &mut [
        (&mut wakeup_pin, read_volatile!(WAKEUP_LEVEL)),
        ];
    let rtcio = RtcioWakeupSource::new(wakeup_pins);

    // send_update = true;
    // write_volatile!(TIMER_SLEEP,true);
    if read_volatile!(SERVER_ADDR) != BROADCAST_ADDRESS && send_update == false  {
        begin_sleep!(rtc, rtcio, timer_wakeup, delay);
    }
    
    
    let timer = SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let _init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();


    let mut esp_now = EspNow::new(&_init,peripherals.WIFI).unwrap();
    esp_now.set_channel(1).unwrap();
    
    println!("My MAC: {:?}",Efuse::get_mac_address());

    

    let server_addr = read_volatile!(SERVER_ADDR);
    if server_addr != BROADCAST_ADDRESS {
        let _ = esp_now.add_peer(PeerInfo {
            peer_address: server_addr,
            lmk: None,
            channel: None,
            encrypt: false,
        });
    } else {
        println!("Sending broadcast information");
        let res =  esp_now.send(&server_addr, &[0xF0,0x00,0x22]).unwrap().wait();
        println!("Send result: {:?}", res);
        
        delay.delay_ms(1000u32);
        let response = esp_now.receive();
        
        match response {
            Some(data) => {
                if data.info.dst_address == Efuse::get_mac_address() {
                    println!("Setting server address to {:?}", data.info.src_address);

                    write_volatile!(SERVER_ADDR,data.info.src_address);
                    send_update = false;
                }
                println!("Received data: {:?}", data);
            },
            None => println!("No data received")
            
        }
    }
    if send_update {
        println!("Sending update to {:?}", server_addr);
        let res =  esp_now.send(&server_addr, &[0x22,wakeup_pin_state as u8]).unwrap().wait();
        println!("Send result: {:?}", res);
    }

    begin_sleep!(rtc, rtcio, timer_wakeup, delay);

}
