# Troubleshooting: Audio Server Plug-in Enumeration

## Issue: Virtual Device Not Appearing in System
Even when the `coreaudiod` or isolated driver process is running, the virtual microphone device (e.g., `translator.virtual.mic.device`) may not appear in the system audio device list.

### Root Causes Found
1.  **AudioServerPlugIn_LoadingConditions in Info.plist**:
    In modern macOS versions (13+), having an `IOService Matching` condition for `IOPlatformExpertDevice` can prevent `coreaudiod` from correctly enumerating the plug-in when it is forced into an isolated driver process.
    - **Fix**: Remove the `AudioServerPlugIn_LoadingConditions` key from `Info.plist`.

2.  **Missing Mandatory Property Handlers**:
    `coreaudiod` is extremely sensitive to missing property handlers in `GetPropertyData` and `GetPropertyDataSize`.
    - **Identified Missing Selectors**:
        - `kAudioPlugInPropertyResourceBundle` ('rsrc')
        - `kPropertyTapList` ('taps')
        - `kAudioObjectPropertyControlList` ('ctrl')
    - **Fix**: Ensure these selectors are explicitly handled in the `switch` statements of `get_property_data_size` and `get_property_data`, even if they return an empty list or 0 size.

3.  **Permissions on Rebuild**:
    Building the bundle multiple times may lead to permission issues in the `build/` directory if files were previously owned or touched by `sudo` during deployment.
    - **Fix**: Use `sudo rm -rf` on the build artifact before running the build script.

## Host Application Troubleshooting

### Issue: Input Level is Zero or Shared Buffer Empty
If the host app starts but the input level (RMS) remains at zero, or the shared buffer file contains only zeros:

1.  **Entitlements**:
    The macOS host application requires specific entitlements to access the microphone and write to the shared buffer when sandboxed.
    - **Required Entitlements**:
        - `com.apple.security.device.microphone`
        - `com.apple.security.app-sandbox`
        - `com.apple.security.files.user-selected.read-write`
    - **Fix**: Re-sign the app with the proper entitlements:
        ```bash
        codesign --force --options runtime --entitlements apps/macos-host/TranslatorVirtualMicHost.entitlements --sign - apps/macos-host/TranslatorVirtualMicHost.app/Contents/MacOS/TranslatorVirtualMicHost
        ```

2.  **Launching from Terminal**:
    Launching the app directly from a terminal can sometimes bypass or complicate TCC (Transparency, Consent, and Control) prompts.
    - **Recommendation**: Launch the app from Finder or via `open apps/macos-host/TranslatorVirtualMicHost.app` after re-signing.

## Standardized Deployment & Reset Flow
If the device disappears after a code change, follow this sequence exactly:

1.  **Clean and Rebuild**:
    ```bash
    sudo rm -rf native/macos/build/TranslatorVirtualMic.driver
    ./native/macos/scripts/build-plugin-bundle.sh
    ```

2.  **Deploy to System Path**:
    ```bash
    sudo APPLY=1 TARGET_ROOT=/ ./native/macos/scripts/deploy-plugin-bundle.sh
    ```

3.  **Hard Reset Audio Services**:
    ```bash
    sudo killall coreaudiod
    # Wait a few seconds for the service to restart
    sleep 3
    ```

4.  **Verify Presence**:
    ```bash
    ./native/macos/scripts/run-hal-smoke-verifier.sh --list
    ```

5.  **Debug Logs**:
    If it still doesn't appear, find the specific driver process PID and tail its logs:
    ```bash
    ps aux | grep "TranslatorVirtualMic.driver"
    # Use the PID found above:
    log show --predicate 'processID == <PID>' --last 5m --style compact --info --debug
    ```
