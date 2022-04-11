use std::time::Duration;
use febft::bft::communication::NodeId;

pub fn start_statistics_thread(node_id: NodeId) {

    std::thread::Builder::new()
        .name(String::from("OS Statistics thread"))
        .spawn(move || {
            loop {
                take_cpu_measurement(node_id);

                std::thread::sleep(Duration::from_millis(250));
            }
        });

}

fn take_cpu_measurement(node_id: NodeId) {
    let result = mprober_lib::cpu::get_all_cpu_utilization_in_percentage(false, Duration::from_millis(100)).unwrap();

    let mut cpu_res = Vec::with_capacity(result.len());

    let mut curr_cpu = 0;

    for usage in result {
        cpu_res.push((curr_cpu, usage));

        curr_cpu += 1;
    }

    let free_mem = mprober_lib::memory::free().unwrap();

    let network_speed = mprober_lib::network::get_networks_with_speed(Duration::from_millis(100)).unwrap();

    let mut tx_speed = 0.0;
    let mut rx_speed = 0.0;

    for (network, speed) in network_speed {
        if speed.receive > rx_speed || speed.transmit > tx_speed {
            rx_speed = speed.receive;
            tx_speed = speed.transmit;
        }
    }

    println!("{:?} // OS Resource usage.", node_id);

    for (cpu_id, usage) in cpu_res {
        println!("{:?} // CPU {} utilization: {}", node_id, cpu_id, usage);
    }

    println!("{:?} // Network TX Usage {} Network RX Usage {}", node_id, tx_speed, rx_speed);

    println!("{:?} // RAM Usage {} Free {} Total", node_id, free_mem.mem.free, free_mem.mem.total);
}