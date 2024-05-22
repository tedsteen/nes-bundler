# Configure your bundle

This directory has a pre-configured bundle for reference.  
If you want to build a proper bundle you also need to dig into the individual subdirectories here to customise installer graphics, icons etc.

Here is a breakdown of what can be customised
* [config.yaml](config.yaml) - the main configuration
* [rom.nes](rom.nes) - your game
* [netplay-rom.nes](netplay-rom.nes) - optional custom Netplay ROM. This will enable a different player experience for netplayers, if f.ex in a netplay session you do not want to present the player with the one player option you can bake a ROM that defaults to two players
* [palette.pal](palette.pal) - the current palette is generated with `palgen_persune.py --skip-plot -aps 5 -ela 0.01429 -e -hue 3.75 -sat 0.8 -o palette.pal`. Look [here](https://github.com/Gumball2415/palgen-persune) for details

The rest is only needed for a proper bundle
* [Linux icon](linux/icon_256x256.png)
* [Mac icon set](macos/bundle.iconset/)
* [Windows app and window title icon](windows/app.ico) look [here](https://learn.microsoft.com/en-us/windows/apps/design/style/iconography/app-icon-construction) for details about baking a proper windows .ico-file
* [Windows installer details](windows/wix/) (banner.bmp, dialog.bmp and licence.rtf)

## Prepare the configuration for bundling

When you are done configuring your bundle you need to zip the files and then it is ready to be sent of for bundling.  
With 7zip it would look like this
```bash
7z a -r config.zip palette.pal config.yaml rom.nes netplay-rom.nes linux/* macos/* windows/*
```

## Full control (you probably don't need this)
If you want to build your own binaries with your own certificates you would have to fork this repository and provide some github secrets to make the builds work.

### Signing the binaries
The GitHub action build scripts will sign the binaries. To do that it needs a couple of secrets.
Currently the macOS bundles and the windows binaries is digitally signed.

#### Secrets needed to sign the macOS bundle

##### BUNDLE_APPLE_TEAM_ID
Your Apple developer Team ID
##### BUNDLE_APPLE_USER
Your Apple ID
##### BUNDLE_APPLE_APP_PASSWORD
App specific password, create it under your account [here](https://appleid.apple.com/account/manage)

##### BUILD_PROVISION_PROFILE_BASE64
A base64 encoded provision profile.  
Create it [here](https://developer.apple.com/account/resources/profiles/list) and then base64 encode it `base64 -i "mygame.provisionprofile" | pbcopy`

##### BUILD_CERTIFICATE_BASE64
A base64 encoded build certificate.  
[Here's a guide](https://support.magplus.com/hc/en-us/articles/203808748-iOS-Creating-a-Distribution-Certificate-and-p12-File) on how to create it.  
Remember the password as you need it for the next secret.  
When you have the p12-file base 64 encode it `base64 -i "certificate.p12" | pbcopy`

##### P12_PASSWORD
The password you created for the build certificate in the previous step.

##### CODE_SIGN_IDENTITY
The code sign identity for the build certificate.  
Run `xcrun security find-identity -v -p codesigning` to find it

#### Secrets needed to sign the Windows binaries

First [read this](https://melatonin.dev/blog/how-to-code-sign-windows-installers-with-an-ev-cert-on-github-actions/)
And if you manage to get through all that you should have the five following variables :)
* AZURE_KEY_VAULT_URI
* AZURE_CLIENT_ID
* AZURE_CLIENT_SECRET
* AZURE_CERT_NAME
* AZURE_TENANT_ID