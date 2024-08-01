# waveform-rs

![image](https://github.com/user-attachments/assets/6f27b950-f0e0-477b-bbeb-76b87b6264dd)

WASM package to create Waveforms in-browser, processing takes around 1-2ms for most samples as opposed to 20-50ms in raw JS

Supports all codecs except for MKV (.webm), thanks to amazing [Project Symfonia](https://docs.rs/symphonia/latest/symphonia/)

## Installation

Available on [NPM.js](https://www.npmjs.com/package/waveform-rs)

```sh
npm i --save-dev waveform-rs
```

## Usage

You need to proc the `init()` method which calls `WebAssembly.instantiate`, it can be wrapped as below to avoid snake_case'ing

```typescript
import init, { audio_to_waveform } from "waveform-rs";

const processAudioData = async (
  data: Uint8Array,
  samplesPerSecond?: number,
): Promise<Float32Array> => {
  await init();
  const waveform = audio_to_waveform(data, samplesPerSecond);
  return waveform;
};
```

In case you are dealing with blobs, you can, the file format is going to be
determined automatically

```typescript
const response = await fetch(sample);
const arrayBuffer = await response.arrayBuffer();
const data = await processAudioData(
  new Uint8Array(arrayBuffer),
  samplesPerSecond,
);
```
