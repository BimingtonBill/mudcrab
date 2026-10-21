# Credits: mods, frameworks and plugins Mudcrab builds on or replaces

One row per mod or framework. Mudcrab never ships any mod's files; it converts the user's own install.

| Name | Author | URL | Licence | What Mudcrab took or replaced |
|---|---|---|---|---|
| Skyrim Script Extender 64 (SKSE64) | Ian Patterson, Stephen Abel, Paul Connelly | https://skse.silverlock.org | SKSE licence, not redistributed | host for the MudcrabCapture reference-capture plugin (tools/capture/skse-plugin); not shipped |
| CommonLibSSE-NG (alandtse fork, from powerof3/CommonLibSSE and CharmedBaryon/CommonLibSSE-NG) | Ryan McKenzie, CharmedBaryon, powerof3, alandtse and contributors | https://github.com/alandtse/CommonLibSSE-NG | GPL-3.0-or-later with the Modding and Linking Exceptions | game types and addresses linked into the capture plugin; the plugin is a local dev tool and is not distributed |
| Address Library for SKSE Plugins | meh321 | https://www.nexusmods.com/skyrimspecialedition/mods/32444 | per its Nexus page, not redistributed | version-independent addresses the capture plugin loads at run time |
| Mod Organizer 2 | MO2 team | https://github.com/ModOrganizer2/modorganizer | GPL-3.0 | launches the game with the plugin through its virtual file system |
| Photo Mode | powerofthree | https://www.nexusmods.com/skyrimspecialedition/mods/91701 | MIT (GitHub powerof3/PhotoMode) | optional, for hand-framing views; nothing copied |
| Community Shaders | doodlum and the Community Shaders team | https://www.nexusmods.com/skyrimspecialedition/mods/86492 | see its repository | installed only for future RenderDoc capture; off for vanilla references |
