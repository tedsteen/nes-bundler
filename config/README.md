# Configure your bundle

This directory has a pre-configured bundle that you can have a look at.  
You also need to dig into the individual subdirectories here to customise installer graphics, icons etc.

Here is a breakdown of what can be customised
* The main configuration [config.yaml](config.yaml)
* [Linux icon](linux/icon_256x256.png)
* [Mac icon set](macos/bundle.iconset/)
* [Windows program and window icon](windows/icon_256x256.ico)
* [Windows installer details](windows/wix/) (banner.bmp, dialog.bmp and licence.rtf)

To make your own bundle, clone this repository and do your updates, push and trigger a build by pushing a tag.