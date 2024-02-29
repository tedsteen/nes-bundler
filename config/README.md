# Configure your bundle

This directory has a pre-configured bundle that you can have a look at.  
You also need to dig into the individual subdirectories here to customise installer graphics, icons etc.

Here is a breakdown of what can be customised
* The main configuration [config.yaml](config.yaml)
* [Linux icon](linux/icon_256x256.png)
* [Mac icon set](macos/bundle.iconset/)
* [Windows program and window icon](windows/icon_256x256.ico)
* [Windows installer details](windows/wix/) (banner.bmp, dialog.bmp and licence.rtf)

# Signing the binaries
The Github action build scripts will sign the binaries. To do that they need a couple of secrets.
Currently only the macOS bundle is digitally signed.

## Secrets needed to sign the macOS bundle

### BUNDLE_APPLE_TEAM_ID
Your Apple developer Team ID
### BUNDLE_APPLE_USER
Your Apple ID
### BUNDLE_APPLE_APP_PASSWORD
App specific password, create it under your account [here](https://appleid.apple.com/account/manage)

### BUILD_PROVISION_PROFILE_BASE64
A base64 encoded provision profile.  
Create it [here](https://developer.apple.com/account/resources/profiles/list) and then base64 encode it `base64 -i "mygame.provisionprofile" | pbcopy`

### BUILD_CERTIFICATE_BASE64
A base64 encoded build certificate.  
[Here's a guide](https://support.magplus.com/hc/en-us/articles/203808748-iOS-Creating-a-Distribution-Certificate-and-p12-File) on how to create it.  
Remember the password as you need it for the next secret.  
When you have the p12-file base 64 encode it `base64 -i "certificate.p12" | pbcopy`

### P12_PASSWORD
The password you created for the build certificate in the previous step.

### CODE_SIGN_IDENTITY
The code sign identity for the build certificate.  
Run `xcrun security find-identity -v -p codesigning` to find it