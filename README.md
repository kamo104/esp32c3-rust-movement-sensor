# No_std ESP32C3 rust esp-now movement sensor

This project is a no_std esp32c3 esp-now end device.

The device sends a broadcast to connect to a gateway that later relays its information to an mqtt server.

Gateway implementation: [esp32-rust-mqtt-esp-now-gateway](https://github.com/kamo104/esp32-rust-mqtt-esp-now-gateway/tree/main)
