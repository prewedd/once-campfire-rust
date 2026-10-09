MediaPipe for the huddle's background blur (lib/huddle/huddle.js), served from here instead of
jsDelivr and Google Cloud Storage:

- `vision_wasm_internal.js`, `vision_wasm_internal.wasm`: @mediapipe/tasks-vision 0.10.14, `wasm/`
  (the SIMD build; every current browser has WebAssembly SIMD). Apache-2.0.
- `selfie_segmenter.tflite`: storage.googleapis.com/mediapipe-models/image_segmenter/selfie_segmenter/float16/latest/
  (sha256 191ac9529ae506ee0beefa6b2c945a172dab9d07d1e802a290a4e4038226658b). Apache-2.0.
