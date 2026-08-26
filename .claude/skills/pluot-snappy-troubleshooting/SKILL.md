---
name: pluot-snappy-troubleshooting
description: "Use when rust build fails due to command did not execute successfully, involving snappy or snappy_src or snappy/snappy.cc"
---

On macOS, if you run into an Xcode SDK conflict (e.g., during `snappy_src` compilation), try setting the `SDKROOT` environment variable to ensure that C and C++ system headers come from the same CommandLineTools SDK version:

```sh
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.1.sdk
```
