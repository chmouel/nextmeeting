# nextmeeting - Show your calendar next meeting in your waybar or polybar

## What is it?

nextmeeting is a simple CLI leveraging gcalcli to show the next meetings.

It has a few features compared to just gcalcli :

- bar integration (i.e: [waybar](https://github.com/Alexays/Waybar)/[polybar](https://github.com/polybar/polybar) and probably others)
- smart date in English (not just the date, tomorrow or others)
- show the time to go for current meeting
- change colors if there is 5 minutes to go to the meeting
- hyperlink in default view to click on terminal
- notification via notify-send 5 minutes before meeting
- title ellipsis
- Exclude next day meetings.

## Screenshot

![192647099-ccfa2002-0db3-4738-a54b-176a03474483](https://user-images.githubusercontent.com/98980/212869786-1acd56e2-2e8a-4255-98c3-ebbb45b28d6e.png)

## How to use it?

You need to install [gcalcli](https://github.com/insanum/gcalcli) and [setup
the google Oauth integration](https://github.com/insanum/gcalcli#login-information) with google calendar.

By default you can start `nextmeeting` and it will show the list of meetings you
have with "human date".

There is a few options to customize things, see `nextmeeting --help` for more.

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
        "tooltip": "true"
    },
```

This will detect if i have my external display connected for the length of the
tile and show how long i have until the next meeting. If if i click on the item
it will open the meet URL attached to the event. On right click it will use
`kitty` terminal to show the time zones with
[batz](https://github.com/chmouel/batzconverter) and my next meeting. I can
click on the title in the terminal and it will open the meet URL.

You can style the waybar item with the following CSS:

```css
#custom-agenda {
  color: #696969;
}
```

If you enable the option "--notify-min-before-events it will output a class
`soon` if the events is coming soon, you can style it with:

```css
#custom-agenda.soon {
  color: #eb4d4b;
}
```

### Installation

Use `pip` with:

`pip install -U nextmeeting`

or Checkout the source of this repo and use `poetry` :

`poetry run nextmeeting`

If you don't want to use poetry you can install the depndences packages from PyPi
or from your operating system package manager if available:

- <https://pypi.org/project/python-dateutil/>
- <https://pypi.org/project/gcalcli/>

And the you can run the nextmeeting script:

`python3 ./nextmeeting/cli.py`

alternatively you can even just copy the `./nextmeeting/cli.py` script to your path and run
it to make it more convenient.

### [AUR](https://aur.archlinux.org/packages/nextmeeting)

```shell
yay -S nextmeeting
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
