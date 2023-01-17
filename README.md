# nextmeeting - Show your calendar next meeting in your waybar or polybar

## What is it?

nextmeeting is a simple cli leveraging gcalcli to show  the next meetings.

It has a few features compared to just gcalcli : 

* bar integration (ie: [waybar](https://github.com/Alexays/Waybar)/[polybar](https://github.com/polybar/polybar) etc..)
* smart date in english (not just the date, tomorrow or others)
* show the time to go for current meeting
* change colors if there is 5 minutes to go to the meeting
* hyperlink in default view to click on terminal
* notificaiton via notify-send 5 minutes before meeting
* title elipsis

## How to use it?

You need to install [gcalcli](https://github.com/insanum/gcalcli) and [setup
the google oauth integration](https://github.com/insanum/gcalcli) with google calendar.

by default you can press nextmeeting and it will show the list of meetins you
have with "human date".

### Waybar 

More interesting to integrate with waybar you can have something like this:

```json
    "custom/agenda": {
        "format": "{}",
        "exec": "size=30;swaymsg -t get_outputs -p |grep -q 'Current mode: 3440x1440' && size=80; nextmeeting --max-title-length ${size} --waybar",
        "on-click": "nextmeeting --open-meet-url",
        "on-click-right": "kitty --class=GClock -- /bin/bash -c \"batz;echo;cal -3;echo;nextmeeting;read;\";",
        "interval": 59,
        "return-type": "json",
        "tooltip": "true",
        "tooltip-format": "{tooltip}",
    },
```

This will detect if i have my external display connected for the lenght of the tile and show how long i have until the next meeting.
If if i click on the item it will open the meet url attached to the event.
On right click it will use `kitty` terminal  to show the timezones with
[batz](https://github.com/chmouel/batzconverter) and my next meeting. I can
click on the title in the terminal and it will open the meet url.

### Installation

Copy the [nextmeeting](./nextmeeting) script somewhere
in your PATH (ie: `~/.local/bin` or `/usr/local/bin`)

For the dependences you will need to install those packages from pypi (pip
install --user package) or from your package manager if available:

* https://pypi.org/project/python-dateutil/
* https://pypi.org/project/gcalcli/

### AUR

```shell
yay -Ss nextmeeting
```

### Related

* For Gnome: [gnome-next-meeting-applet](https://github.com/chmouel/gnome-next-meeting-applet)

## Copyright

[Apache-2.0](./LICENSE)

## Authors

- Chmouel Boudjnah <https://github.com/chmouel>
    - Fediverse - <[@chmouel@fosstodon.org](https://fosstodon.org/@chmouel)>
    - Twitter - <[@chmouel](https://twitter.com/chmouel)>
    - Blog  - <[https://blog.chmouel.com](https://blog.chmouel.com)>
