# Temporal pixelsorting

Useful for artistic nonsense like removing moving objects from time-lapses, reviewing security camera footage, or if for some reason you want to know N-th percentile of every pixel of the frame over time.

## Caveats

- Unlike `ffmpeg`, this will not ask before overwriting output file.
- This will use a lot of RAM (specifically, at least width × height × 3072 bytes of it) regardless of file duration.

You have been warned.

## Usage

```
temporal-pixelsort ampijbp1ttxe1.mp4 -- -c:v libx264 -vf format=yuv420p -movflags +faststart ampijbp1ttxe1-psorted.mp4
```

[Sample input and output side by side](ampijbp1ttxe1.mp4).

## Dependencies

`ffmpeg` and `ffprobe` binaries.
