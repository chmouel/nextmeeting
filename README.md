# nextmeeting - Show your calendar next meeting in your waybar or polybar

## What is it?

nextmeeting is a simple CLI tool that leverages `gcalcli` to display your
upcoming meetings.

It offers several features beyond basic `gcalcli` functionality:

- **Bar Integration:** Seamlessly integrates with status bars like
  [Waybar](https://github.com/Alexays/Waybar) and [Polybar](https://github.com/polybar/polybar).
- **Smart Date Display:** Shows dates in a human-readable English format (e.g.,
  "tomorrow," "next Monday," not just raw dates).
- **Time-to-Meeting:** Displays the remaining time until the current meeting starts.
- **Color-Coded Alerts:** Changes colors when a meeting is 5 minutes away.
- **Hyperlink Support:** Provides clickable hyperlinks in the default terminal view.
- **Meeting Notifications:** Sends notifications via `notify-send` 5 minutes
  before a meeting.
- **Title Ellipsis:** Truncates long meeting titles for better display.
- **Next-Day Exclusion:** Option to exclude meetings scheduled for the next day.

## Screenshot

![192647099-ccfa2002-0db3-4738-a54b-176a03474483](https://user-images.githubusercontent.com/98980/212869786-1acd56e2-2e8a-4255-98c3-ebbb45b28d6e.png)

## Installation

Use `pip` with:

`pip install -U nextmeeting`

Alternatively, if you prefer to run from source, you can use `uv` (recommended)
or install dependencies manually.

### Using uv (recommended)

First, install `uv` by following the instructions [uv installation
guide](https://docs.astral.sh/uv/getting-started/installation/). Then, clone
this repository and run:

```shell
uv run nextmeeting
```

### Manual Installation

If you don't want to use `uv`, you can install the dependencies manually from
PyPI or your operating system's package manager:

- [python-dateutil](https://pypi.org/project/python-dateutil/)
- [gcalcli](https://pypi.org/project/gcalcli/)

After installing dependencies, you can run the `nextmeeting` script directly:

```shell
python3 src/nextmeeting/cli.py
```

You can also copy `src/nextmeeting/cli.py` to your system's PATH for convenience.

### [AUR](https://aur.archlinux.org/packages/nextmeeting)

```shell
yay -S nextmeeting
```

### NixOS

<details><summary>Flake and Home-Manager install instructions.</summary>

- Add nextmeeting to your flake.

```nix
nextmeeting = {
  url = "github:chmouel/nextmeeting?dir=packaging";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

- Use Home-manager to add nextmeeting to waybar like this:

```nix
let 
  nextmeeting = lib.getExe inputs.nextmeeting.packages.${pkgs.system}.default;
in
{
  "custom/agenda" = {
      format = "{}";
      exec = nextmeeting + "--max-title-length 30 --waybar";
      on-click = nextmeeting + "--open-meet-url";
      interval = 59;
      return-type = "json";
      tooltip = true;
  };
}
```

- Follow along with the rest of the instructions.

</details>

## How to use it?

You need to install the [gcalcli](https://github.com/insanum/gcalcli) tool and
[setup the google Oauth
integration](https://github.com/insanum/gcalcli?tab=readme-ov-file#initial-setup)
with google calendar.

By default, you can start `nextmeeting`, and it will display your list of
meetings with a human-readable date format.

If no meetings are displayed, you might need to specify the target calendar
using the `--calendar=CALENDAR` flag.

There are a few options to customize its behavior; see `nextmeeting --help` for
more details.

### Waybar

A more interesting use case for `nextmeeting` is its integration with Waybar,
allowing for a clean output on your desktop. For example, my configuration
looks like this:

```json
    "custom/agenda": {
        "format": "{}",
        "exec": "nextmeeting --max-title-length 30 --waybar",
        "on-click": "nextmeeting --open-meet-url",
        "on-click-right": "kitty -- /bin/bash -c \"batz;echo;cal -3;echo;nextmeeting;read;\"",
        "interval": 59,
        "return-type": "json",
        "tooltip": "true"
    },
```

This configuration displays the time remaining until my next meeting. Clicking
the item opens the meeting's URL. A right-click launches a `kitty` terminal to
show time zones using [batz](https://github.com/chmouel/batzconverter) and my
next meeting. I can also click on the meeting title within the terminal to open
its URL.

#### Styling

You can style the Waybar item using the following CSS:

```css
#custom-agenda {
  color: #696969;
}
```

If you enable the `--notify-min-before-events` option, `nextmeeting` will
output a `soon` class when an event is approaching, allowing you to style it
with:

```css
#custom-agenda.soon {
  color: #eb4d4b;
}
```

### Related

- For Gnome: [gnome-next-meeting-applet](https://github.com/chmouel/gnome-next-meeting-applet)

## Copyright

[Apache-2.0](./LICENSE)

## Authors

- Chmouel Boudjnah <https://github.com/chmouel>
  - Fediverse - <[@chmouel@fosstodon.org](https://fosstodon.org/@chmouel)>
  - Twitter - <[@chmouel](https://twitter.com/chmouel)>
  - Blog - <[https://blog.chmouel.com](https://blog.chmouel.com)>
