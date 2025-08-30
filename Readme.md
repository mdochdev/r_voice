# ESP32S3

This is using a freenove-esp32s3-wroom board. But any chip esp32s3 will work. It is configured to 16Mb. Hardcoded in .cargo/config.toml and also in the partitions.csv and a few other locations. You will need to fix them all for a smaller board.

# ESP-SR

The sound libraries (AFE, VAD etc) are only supported on a few esp32 chipsets ESP32, ESP32S3 and ESP32P4. There could be others but check before you try something else.

# INMP441 Mic.

This example expects a INMP441 mic to be connected to the ports defined in main. If you plug into others change as needed. The mic is 24-bits, when used with the I2S it needs to be read as 32-bit chunks and shifted. Currently configured in mono mode with L/R -> GND


# References

I used this working reference written in C which I updated to the latest libraries and wn7_hiesp, mn6_en.
https://github.com/devweirdo/s3-sr

The really interesting code is how the bits are shifted.
