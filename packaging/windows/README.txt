
Prerequisite for packaging:

* You need Wix 4 tools installed, probably with DOTNET installed first.
* Copy the gossip.exe here (release build)
* Copy the gossip.png here
* For new versions of gossup, update gossip.wxs
	* UPDATE the Package ProductCode GUID to a new one
    * KEEP the UpgradeCode GUID (it should never change, it ties different versions together)
    * Change a component GUID ONLY IF the absolute path changes.

Compile:

  $ cargo build --release

Copy the binary to the packaging diretory

  $ cp ..\..\target\release\gossip.exec .

Packaging:

  $ wix build gossip.wxs

Move to a versioned filename:

  $ mv gossip.msi gossip.VERSION.msi

Upload to github releases.


----
To install the package, either double-click the MSI, or

  $ msiexec gossip.msi

To remove the package from your windows computer:

  $ msiexec /x gossip.msi
