use std::ffi::{CStr, CString};
use std::sync::Arc;
use std::thread;

use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::AnyIOPin;
use esp_idf_hal::i2s::{I2sDriver, I2sRx}; 
use esp_idf_hal::i2s::config::{ClockSource, Config, DataBitWidth, MclkMultiple, SlotMode, StdClkConfig, StdConfig, StdGpioConfig, StdSlotConfig}; 
use esp_idf_hal::prelude::Peripherals;
use esp_idf_sys::EspError;
use esp_idf_sys::esp_sr::*;

fn main() -> Result<(), EspError> {

    println!("Starting main");
    // IDF runtime glue
    esp_idf_svc::sys::link_patches();

    // ---------- I2S (2× INMP441 on one bus: L and R) ----------
    let p = Peripherals::take().unwrap();
    let pins = p.pins;

    // Adjust to your board:
    let bclk = pins.gpio13; // BCLK/SCK
    let ws = pins.gpio12;   // LRCLK/WS
    let din = pins.gpio14; // SD (mic data -> ESP)  // moved off GPIO6
    let mclk = AnyIOPin::none();
    let i2s = p.i2s0;


    //let config = StdConfig::philips(16000, DataBitWidth::Bits32);
    println!("Setting config");
    let channel_cfg = Config::default();
    let clk_cfg = StdClkConfig::new(16000, ClockSource::default(), MclkMultiple::M256);
    let slot_cfg = StdSlotConfig::philips_slot_default(DataBitWidth::Bits32, SlotMode::Mono);
    let gpio_cfg = StdGpioConfig::new(false, false, false);
    let config = StdConfig::new(channel_cfg, clk_cfg, slot_cfg, gpio_cfg);
    let mut rx_handle = I2sDriver::new_std_rx(i2s, &config, bclk, din, mclk, ws)?;
    println!("Init handle");
    let _r = rx_handle.rx_enable()?;
    let ctx = get_afe_data();
    let ctx = Arc::new(ctx);

    // process task (VAD/DOA/WakeNet)      
    let t1 = {
        let ctx_1 = Arc::clone(&ctx);
        thread::Builder::new()
            .name("afe_proc".into())
            .stack_size(8 * 1024) // <-- add
            .spawn(move || process_task(&ctx_1) )
            .expect("spawn proc failed")
    };
    
    // feed task (owns the I2S driver)
    let t2 = {
        let ctx_2 = Arc::clone(&ctx);
        println!("Enabling afe_feed thread");
        thread::Builder::new()
            .name("afe_feed".into())
            .stack_size(8 * 1024) // <-- add
            .spawn(move || feed_task(rx_handle, &ctx_2))
            .expect("spawn feed failed")
    };

    
    let _r = t1.join().unwrap();
    let _r = t2.join().unwrap()?;

    loop {
        FreeRtos::delay_ms(1000);
    }

}

fn get_models() -> *mut srmodel_list_t {
    unsafe  { esp_srmodel_init(CString::new("model").unwrap().as_ptr())}
}

fn get_afe_data() -> Ctx {
    unsafe {
        let models = get_models();
        let afe_config = afe_config_init(CString::new("MNNN").unwrap().as_ptr(), models, afe_type_t_AFE_TYPE_SR, afe_mode_t_AFE_MODE_HIGH_PERF);
        (*afe_config).wakenet_model_name = CString::new("wn9s_hiesp").unwrap().into_raw();
        (*afe_config).aec_init      = false;
        (*afe_config).pcm_config.total_ch_num = 2;
        (*afe_config).pcm_config.mic_num = 1;
        (*afe_config).pcm_config.ref_num = 1;

        let afe_handle = esp_afe_handle_from_config(afe_config);
        let afe_data = (*afe_handle).create_from_config.expect("create_from_config")(afe_config);

        Ctx {
            afe_handle,
            afe_data,
        }
    }
}

#[derive(Clone)]
struct Ctx {
    // AFE
    afe_handle: *mut esp_afe_sr_iface_t, // your iface is a *mut in bindings
    afe_data: *mut esp_afe_sr_data_t,
}

unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

impl Ctx {
    fn get_feed_chunk_size(&self) -> i32 {
        unsafe {
            (*self.afe_handle).get_feed_chunksize.expect("get_feed_chunk_size")(self.afe_data) 
        }
    }

    fn get_fetch_chunk_size(&self) -> i32 {
        unsafe {
            (*self.afe_handle).get_fetch_chunksize.expect("get_fetch_chunk_size")(self.afe_data)
        }
    }

    fn get_channel_num(&self) -> i32 {
        unsafe { 
            (*self.afe_handle).get_channel_num.expect("get_channel_num")(self.afe_data)
        }
    }
    fn feed(&self, buffer: Vec<i16>) {
        unsafe { 
            (*self.afe_handle).feed.expect("feed")(self.afe_data, buffer.as_ptr() as _);
        }
    }
    fn enable_wakenet(&self) {
        unsafe {
            (*self.afe_handle).enable_wakenet.expect("Enable wakenet")(self.afe_data);
        }
    }
}

fn feed_task(mut rx: I2sDriver<'static, I2sRx>, ctx: &Arc<Ctx>) -> Result<(), EspError> {
    
    println!("In feed task");
    
        let audio_chunksize = ctx.get_feed_chunk_size();
        println!("Audio Chunk size {}", audio_chunksize);
        let nch = ctx.get_channel_num();
        println!("nch {}", nch);
        //let feed_channel: usize = 2;
        
        let mut buffer: Vec<u8> = vec![0u8; (audio_chunksize * 2 * nch) as usize];
        //let mut u16_buf: Vec<u16> = vec![0u16; (audio_chunksize * nch) as usize];
        println!("buffer len {}", buffer.len());
        loop {
            // Buffer is always full unless timeout is hit.
            let _read = rx.read(&mut buffer, 1000)?;

            let i16_buf = get_bits(&buffer);
            ctx.feed(i16_buf);
        }
}


fn process_task(ctx: &Arc<Ctx>) {
    let mut detect_flag = false;
    let mn_name = CString::new("mn6_en").unwrap();
    let multinet       = unsafe {esp_mn_handle_from_name(mn_name.clone().into_raw())};
    let model_data = unsafe {(*multinet).create.expect("create")(mn_name.as_ptr(), 6000)};
    let mu_chunksize               = unsafe {(*multinet).get_samp_chunksize.expect("get_samp_chunk")(model_data)};
    let afe_chunksize           = ctx.get_fetch_chunk_size();

    assert!(mu_chunksize == afe_chunksize);
    /* 
    unsafe {
        esp_mn_commands_alloc(multinet, model_data);
        esp_mn_commands_add(1,  CString::new("HELLO").unwrap().as_ptr());
        esp_mn_commands_add(2,  CString::new("MAKE ME A COFFEE").unwrap().as_ptr());
        esp_mn_commands_add(3,  CString::new("FAN SPEED").unwrap().as_ptr());
        esp_mn_commands_update();
    }
    */
    unsafe{(*multinet).print_active_speech_commands.expect("Print active speech commands")(model_data)};
    println!("Enabling wakenet");
    //(*ctx.afe_handle).enable_wakenet.expect("Enable wakenet")(ctx.afe_data);

    println!("------------detect start------------\n");
    loop {
        let res = unsafe {
            (*ctx.afe_handle).fetch.expect("Fetch")(ctx.afe_data)
        };

        if unsafe {(*res).wakeup_state} == wakenet_state_t_WAKENET_DETECTED {
            println!("WAKEWORD DETECTED\n");
            unsafe {(*multinet).clean.expect("Clean")(model_data)};
        }

        if unsafe {(*res).raw_data_channels}  == 1 && unsafe {(*res).wakeup_state} == wakenet_state_t_WAKENET_DETECTED {
            detect_flag = true;
        } else if unsafe {(*res).raw_data_channels} > 1 &&unsafe{(*res).wakeup_state} == wakenet_state_t_WAKENET_CHANNEL_VERIFIED {
            detect_flag = true;
            println!("AFE_FETCH_CHANNEL_VERIFIED, channel index: {:?}", unsafe{(*res).trigger_channel_id});
        }

        if detect_flag {
            let mn_state = unsafe{(*multinet).detect.expect("Dectect(")(model_data, (*res).data)};

            if mn_state == esp_mn_state_t_ESP_MN_STATE_DETECTING {
                continue;
            }

            if mn_state == esp_mn_state_t_ESP_MN_STATE_DETECTED {
                let mn_result = unsafe{(*multinet).get_results.expect("Get results")(model_data)};
                for i in 0..unsafe{(*mn_result).num} {
                    unsafe{
                        println!(
                        "TOP {}, command_id: {:?}, phrase_id: {:?}, string: {:?}, prob: {:?}",
                        i + 1, (*mn_result).command_id, (*mn_result).phrase_id, CStr::from_bytes_until_nul(&(*mn_result).string), (*mn_result).prob);
                    }
                }

                // Take action when detected
                unsafe{println!("{:?}", CStr::from_bytes_until_nul(&(*mn_result).string))};
                //sr_detect_action_execute(mn_result->command_id[0], mn_result->phrase_id[0], mn_result->string, mn_result->prob[0]);
                println!("-----------listening-----------");
            }

            if mn_state == esp_mn_state_t_ESP_MN_STATE_TIMEOUT {
                let mn_result = unsafe{(*multinet).get_results.expect("Get Results")(model_data)};
                unsafe {println!("timeout, string: {:?}", CStr::from_bytes_until_nul(&(*mn_result).string));}
                ctx.enable_wakenet();
                detect_flag = false;
                println!("-----------awaits to be waken up-----------");
                continue;
            }
        }

    }
}

fn get_bits(input: &Vec<u8>) -> Vec<i16> {
    assert!(input.len() % 4 == 0, "Length must be divisible by 4");

    let mut result = Vec::new();

    for chunk in input.chunks_exact(4) {
        let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let shifted = val >> 14;

        // Split into high 16 bits and low 16 bits
        let high = ((shifted >> 16) & 0xFFFF) as i16;
        let low  = (shifted & 0xFFFF) as i16;

        result.push(low);
        result.push(high);
    }    
    result

}
