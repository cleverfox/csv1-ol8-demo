use std::io::{self, Write};
use std::time::Duration;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    //let port_name = "/dev/cu.usbmodemcsv1_00011";
    let port_name = args[1].clone();
    println!("serial {port_name}");
    //let port_name = "/dev/cu.usbserial-1410";
    let baud_rate = 115200;
    let rate = 10;

    let builder = serialport::new(&port_name, baud_rate).timeout(Duration::from_millis(10));
    println!("{:?}", &builder);
    let mut port = builder.open().unwrap_or_else(|e| {
        eprintln!("Failed to open \"{}\". Error: {}", port_name, e);
        ::std::process::exit(1);
    });

    println!(
        "Writing to {} at {} baud at {}Hz",
        &port_name, &baud_rate, &rate
    );
    let mut serial_buf: Vec<u8> = vec![0; 1000];
    // 0 - DAC [0..15] or table number [16..20]
    // 1 - table item index [0..255]
    // 2 - word's MSB
    // 3 - word's LSB
    /* + -----------------------------------------------+
     * | First byte  | Second byte  | third & 4th bytes |
     * + -----------------------------------------------+
     * | n = 0..7    | 0x00         | vv                | DirectWrite DAC(n)=vv
     * | n = 0..7    | i+16 (16..19)| vv                | AttachTable DAC(n)=Table(i)
     * | i+16(16..19)| n (0..255)   | vv                | Table(i)[n]=vv
     * | 0xff        | n (0..255)   | 0x0000            | UseTable
     * | 0xfe        | n (0..7)     | 0x0000..0x0001    | control GPIOn
     * | 0xfd        | 0x00         | 0x0000            | KeepAlive (to avoid disabling GPIO0)
     * | 0xfc        | 0x00         | 0x0000            | LDAC - update DACs with loaded values
     * + -----------------------------------------------+
     */

    println!(
        "wr gpio 0 {:?}",
        port.write(&[0xfe, 0, 0, 1, 0xfe, 1, 0, 1])
    );
    println!("read {:?}", port.read(serial_buf.as_mut_slice()));
    let r = port.write(&[16, 49, 0, 0, 16, 50, 64, 0, 16, 51, 128, 0]);
    println!("init1 {:?}", r);
    println!("read {:?}", port.read(serial_buf.as_mut_slice()));
    let r = port.write(&[17, 49, 64, 0, 17, 50, 128, 0, 17, 51, 0, 0]);
    println!("init2 {:?}", r);
    println!("read {:?}", port.read(serial_buf.as_mut_slice()));

    let r = port.write(&[
        0, 16, 0, 0, 1, 17, 0, 0, 2, 16, 0, 0, 3, 17, 0, 0, 4, 16, 0, 0, 5, 17, 0, 0, 6, 16, 0, 0,
        7, 17, 0, 0,
    ]);
    println!("init3 {:?}", r);
    println!("read {:?}", port.read(serial_buf.as_mut_slice()));

    for _ in 0..3 {
        println!("wr keepalive {:?}", port.write(&[0xfd, 0, 0, 0]));
        println!("read {:?}", port.read(serial_buf.as_mut_slice()));
        std::thread::sleep(Duration::from_secs(5));
    }

    let mut msg: Vec<u8> = vec![255, 0, 0, 0];
    let mut v: u16 = 0;
    let mut c: u8 = 0;
    loop {
        c += 1;
        if c > 7 {
            c = 0;
        }

        if v == 65535 {
            v = 0
        } else if v < 65535 - 511 {
            v += 511;
        } else {
            v = 65535;
        }
        msg[0] = c;
        if c < 4 {
            msg[2] = ((v & 0xff00) >> 8) as u8;
            msg[3] = (v & 0xff) as u8;
        } else {
            msg[2] = (((65535 - v) & 0xff00) >> 8) as u8;
            msg[3] = ((65535 - v) & 0xff) as u8;
        }
        //msg[0] = 255;
        //msg[1] = c;

        let r = port.write(&msg);
        println!("write {:?} {:?}", &msg, r);
        match r {
            //match port.write(string.as_bytes()) {
            Ok(_) => {
                std::io::stdout().flush().unwrap();
                match port.read(serial_buf.as_mut_slice()) {
                    Ok(t) => {
                        //io::stdout().write_all(&serial_buf[..t]).unwrap();
                        //io::stdout().flush().unwrap();
                        println!("read {:?}", &serial_buf[..t])
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => eprintln!("{:?}", e),
        }
        if rate == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis((1000.0 / (rate as f32)) as u64));
    }
}
