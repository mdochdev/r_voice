# Overview

This is an example in rust how to import other components not already imported by esp-idf-sys. In this example esp-sr is imported, models are added in a custom partition. NB: esp-idf-sys does this under the covers in the rust build directory. So there is a little bit of path magic that occurs.

Apart from the usual `cargo run` first time round you will need to build and flash the models. Simplest way is `cargo full-flash`, but the running app may not find the model first run due to how espflash works. Either reboot the device or reflash/reboot with `cargo run`
Everything should just work from there.

# WakeNet and MultiNet
By importing esp-sr we get access to WakeNet and MultiNet actions as well as other cool stuff like DOA, AFE, VAD etc... This example is the basics almost exactly like the skainet multinet example in C. 


## ESP32S3
This is using a freenove-esp32s3-wroom board. But any chip esp32s3 will work. It is configured to 16Mb flash, hardcoded in .cargo/config.toml and also in the partitions.csv and a few other locations. You will need to fix them all for a smaller memory or different type of chip.
Only a few chips are supported for AFE so check the espressif website for support.

## INMP441 Mic.
This example expects a INMP441 mic to be connected to the ports defined in main. If you plug into others change as needed. The mic is 24-bits, when used with the I2S it needs to be read as 32-bit chunks and shifted. Currently configured with a single mic in mono mode IE: L/R -> GND


# References
Check out:

esp-idf-sys for what files are needed to modify a build.
esp-sr for what it is
esp-skainet for other c examples you can attempt to port.
Also this for how to shift the bits for a INMP441: https://github.com/devweirdo/s3-sr

Join the rust espressif matrix chat mentioned in the esp-idf-sys repo.

