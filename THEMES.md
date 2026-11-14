# Making a theme
A theme is one CSS file. The overlay page (style.css) provides the layout, and a theme changes the look by setting CSS variables, and optionally by adding its own CSS rules.

- Put your theme in *static/themes/custom/* (your own themes), or see the ones in *static/themes/examples/* and *static/themes/styles/*. 
- Use it by adding *?theme=NAME* to the overlay URL, for example *http://localhost:5000/overlay.html?theme=neon*. The name is the file name without the *.css*.
- Custom themes win over styles, and styles win over examples, if two have the same name.
- Open the Theme Gallery (*/gallery.html*) to preview it. Hard refresh (Ctrl+Shift+R) after editing.

Every variable has a fallback, so a theme only needs to set the ones it wants to change.

## Variables
Set these in *:root { ... }* in your theme file.

### Text
| Variable | Default | What it does |
|---|---|---|
| `--font` | `sans-serif` | Font family for all text |
| `--text` | `white` | Text color |
| `--text-shadow` | `1px 1px 3px black` | Shadow behind the text, see *Shadows and Glows* for settings |
| `--text-size` | `20px` | Default size of artist and album |
| `--text-align` | `left` | Text alignment |
| `--weight` | `700` | Font weight | 
| `--secondary-opacity` | `0.85` | Opacity of artist and album | 

### Title, Artist, and Album

| Variable | Default | What it does |
|---|---|---|
| `--title-color` | `--accent, or white` | Title Color |
| `--title-size` | `32px` | Title Size | 
| `--title-transform` | none | For example *uppercase* | 
| `--title-spacing` | normal | Letter spacing | 
| `--artist-size` | `--text-size` | Artist size |
| `--album-size` | `--text-size` | Album size | 
| `--marquee-speed` | `40` | Scrolling speed for long text, in pixels per second | 
| `--marquee-pause` | 2 | Seconds to wait before each scroll | 

Long text scrolls on its own when it doesn't fit.

### Panel

| Variable | Default | What it does |
|---|---|---|
| `--overlay-width` | `620px` | Width of the overlay |
| `--overlay-height` | `180px` | Height of the overlay | 
| `--panel-bg` | `rgba(10, 10, 10, 0.85)` | Background (a color or a gradient) | 
| `--padding` | `12px` | Space inside the panel | 
| `--gap` | `12px` | Space between the cover and the text |
| `--border` | `none` | For example *2px solid white* | 
| `--radius` | `0` | Corner radius | 
| `--clip` | `none` | A *clip-path* for odd shapes |
| `--offset` | `0px` | Empty space around the overlay, so glows and shadows aren't cut off at the edge of the page. The OBS source needs to be bigger by twice this amount in each direction. | 

### Artwork

| Variable | Default | What it does |
|---|---|---|
| `--artwork-width` | `160px` | Cover width |
| `--artwork-height` | `160px` | Cover height | 
| `--artwork-radius` | `8px` | Use *50%* for a circle | 
| `--artwork-shadow` | `none` | Shadow or glow around the cover | 
| `--artwork-border` | `none` | Border around the cover |

### Progress Bar and Time

| Variable | Default | What it does |
|---|---|---|
| `--accent` | `white` | Main color, used by the title and bar when they aren't set |
| `--bar-color` | `--accent` | Color of the filled part | 
| `--track` | `rgba(255, 255, 255, 0.3)` | Color of the empty part | 
| `--bar-height` | `8px` | Bar thickness | 
| `--bar-radius` | `0` | Bar corner radius |
| `--time-size` | `18px` | Size of the time text |
| `--time-color` | `--text` | Color of the time text | 

### Popup mode

A popup theme stays hidden and shows up only when the song changes. 

| Variable | Default | What it does |
|---|---|---|
| `--popup` | `off` | Set to *1* to turn popup mode on |
| `--popup-card` | `album` | The first card: *album* shows the album name, *none* skips it, or any text in quotes, for example *"NOW PLAYING"* | 
| `--popup-card-time` | `2.5` | Seconds the first card is shown | 
| `--popup-show-time` | `7` | Seconds the title and artist show | 

The album card is skipped when there is no album. Spotify does not provide one, so Spotify songs go straight to the title and artist. For a single named after the song, the card shows the original album name, for example "*Song - Single*".

### Music Reaction

| Variable | Default | What it does |
|---|---|---|
| `--spectrum` | `off` | Set to *1* to ask for the music levels, so the theme can use *--energy* (see below) |
| `--ring-bars` | `off` | Set to a number, for example *48*, to add a ring of that many bars around a record (for themes that use it) | 

The equalizer (*#eq*) turns the music levels on by itself when a theme shows it.

## Values the overlay sets for you
Don't set these in a theme. Read them in your CSS.

| Name | What it is |
|---|---|
| `--energy` | Bass level, 0 to 1. Good for glows and pulses: *box-shadow: 0 0 calc(10px + var(--energy) * 30px) red;* |
| `--progress` | How far into the song, 0 to 1 |
| `--cover` | The cover as an image, for example *background-image: var(--cover);* (good for blurred backgrounds) |
| `--level` | Height of each equalizer bar | 
| `data-playing` on `overlay` | *1* while playing, *0* while paused | 
| `data-skip` on `overlay` | Present for a moment right after the song changes | 
| `data-phase` on `overlay` | *visible*, on for popup themes *station*, *track*, and *hidden* |

Example: *#overlay[data-playing="0"] #title { opacity: 0.5}* dims the title while paused. 

## Elements you can style
Besides the variables, you can write normal CSS for these.

| Element | What it is |
|---|---|
| `#overlay` | The whole panel (*::before* and *::after* are free for extra shapes) |
| `#artwork` | The cover |
| `#song-info` | The text column (*::before* and *::after* are free too) |
| `#title`, `#artist`, `#album` | The text lines | 
| `#line` | Title and artist on one line (hidden unless a theme shows it) | 
| `#progress`, `#progress-bar` | The bar and its fill | 
| `#time`, `#time-elapsed`, `#time-total` | The times (hidden unless a theme shows it) |
| `#eq` | The equalizer (hidden unless a theme shows it) |
| `#label`, `#station` | Spare elements, hidden by default. Themes use them for extra shapes or text. |
| `#extra-1` to `#extra-4` | Four spare, empty elements for drawing shapes. |

Hidden elements stay hidden until your theme shows them, for example *#time { display: block; }*

## Tips
- **Cut-off glows**. Anything drawn outside the overlay is cut off at the edge of the page. Set *--offset* to give it room, and make the OBS browser source bigger by the same amount.
- **Heavy effects**. Big blurs and many animated layers use CPU in OBS. A gradient looks the sames as blur for glows and costs far less.
- **Blur in the gallery**. Blur filters can flicker in Firefox's gallery previews. They are fine in OBS.
- **Test backgrounds**. The gallery has color swatches (dark, black, white, green, checker) to check transparent themes.
- **Fonts**. Put font files in a fonts folder and load them with *font-face* at the top of your theme.
- **Sharing**. A theme is one file, so you can share it by sending the *.css* file.