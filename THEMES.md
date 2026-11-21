# Making a theme
A theme is one CSS file. The overlay page (style.css) provides the layout, and a theme changes the look by setting CSS variables, and optionally by adding its own CSS rules.

- Put your theme in *static/themes/custom/* (your own themes), or see the ones in *static/themes/examples/*, *static/themes/styles/* and *static/themes/game-inspired/*. Any new folder in *static/themes/* also works and gets its own section in the Theme Gallery.
- Use it by adding *?theme=NAME* to the overlay URL, for example *http://localhost:5000/overlay.html?theme=neon*. The name is the file name without the *.css*, and can use only letters, numbers, `-` and `_` (no spaces or extra dots).
- If two themes have the same name, the first one found is used, custom first, examples last, and the other folders in alphabetical order.
- Open the Theme Gallery (*/gallery.html*) to preview it. Refresh the page after adding a theme, and hard refresh (Ctrl+Shift+R) after editing one.

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
| `--title-color` | `--accent`, or `white` | Title Color |
| `--title-size` | `32px` | Title Size | 
| `--title-transform` | `none` | For example *uppercase* | 
| `--title-spacing` | `normal` | Letter spacing | 
| `--artist-size` | `--text-size` | Artist size |
| `--album-size` | `--text-size` | Album size | 
| `--marquee-speed` | `40` | Scrolling speed for long text, in pixels per second | 
| `--marquee-pause` | `2` | Seconds to wait before each scroll | 

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
| `--offset` | `0px` | Empty space around the overlay on every side, so glows and shadows aren't cut off at the edge of the page. The OBS source needs to be bigger than the overlay by twice this amount in width and in height. | 

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

### Popup Mode

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
| `data-playing` on `#overlay` | *1* while playing, *0* while paused | 
| `data-live="1"` on `#overlay` | The song has no end time (live radio, streams). The progress bar and times are hidden by default |
| `data-skip` on `#overlay` | Present for a moment right after the song changes | 
| `data-phase` on `#overlay` | *visible* normally. For popup themes it goes through *station* (the first card), *track* (the title and artist), and *hidden*. |

Example: *#overlay[data-playing="0"] #title { opacity: 0.5; }* dims the title while paused. 

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
| `#extra-1` to `#extra-8` | Eight spare, empty elements for drawing shapes. Each one also has *::before* and *::after*, so you can draw up to 24 more shapes. |

Hidden elements stay hidden until your theme shows them, for example *#time { display: block; }*

### Drawing with the spare elements
The `#extra-N` elements are hidden by default. To use one, show it and place it yourself:

*#extra-1 { display: block; position: absolute; left: 20px; top: 20px; width: 100px; height: 60px; background: red; }*

Their positions are measured from the top left corner of `#overlay`. A *::before* or *::after* on any element needs *content: ""* or it won't appear.

## Shadows and Glows
Shadows are used in three places: `--text-shadow` (the text), `--artwork-shadow` (the cover), and any *box-shadow* you write in your own rules (the panel, the bar, the spare elements).

### Text Shadow
A text shadow has four parts, in this order:

| Part | What it does |
|---|---|
| x offset | Moves the shadow sideways. Positive is right, negative is left |
| y offset | Moves the shadow up or down. Positive is down, negative is up |
| blur | How soft the edge is. *0* is a hard edge, bigger is softer |
| color | The color, including transparency, for example *rgba(0, 0, 0, 0.6)* |

Example: *--text-shadow: 1px 1px 3px black;*

### Box Shadow and Artwork Shadow
A box shadow has one more part, which goes after the blur:

| Part | What it does |
|---|---|
| spread | Makes the shadow bigger (positive) or smaller (negative) than the box |
| inset | Put this word first to draw the shadow inside the box instead of outside |

Example: *--artwork-shadow: 0 4px 12px 2px rgba(0, 0, 0, 0.5);*

### Glows
A glow is a shadow with no offset, so it spreads evenly in every direction: *0 0 12px cyan*. A bigger blur gives a softer, wider glow.

### Several shadows at once
Separate shadows with commas. The first one is drawn on top. A bright, tight shadow with wider ones behind it makes a neon look:

*--text-shadow: 0 0 4px #fff, 0 0 12px #0ff, 0 0 28px #0ff;*

### Glows that follow the music
The overlay sets *--energy* (0 to 1, the bass level). Use it to make a glow pulse:

*#overlay { box-shadow: 0 0 calc(10px + var(--energy) * 30px) red; }*

### Things to watch for
- **Glows outside the overlay are cut off.** See *--offset* and the *Tips* section.
- **Text glows can be clipped.** The title, artist, and album boxes hide anything outside them so long text can scroll. A large text glow gets cut off at the box edges. Add vertical padding and cancel it with a negative margin, for example *#title, #artist, #album { padding: 8px 0; margin: -8px 0; }*. Use vertical padding only, because horizontal padding changes when the text starts to scroll.
- **A *clip-path* cuts shadows too.** A shadow is drawn outside the shape, so anything with *--clip* or *clip-path* won't show its shadow.
- **A soft gradient is cheaper than a blur.** *radial-gradient(closest-side, cyan, transparent)* on a spare element makes a glow that costs much less CPU than a big blur or shadow.

## Tips
- **Cut-off glows**. Anything drawn outside the overlay is cut off at the edge of the page. Set *--offset* to give it room, and make the OBS browser source bigger by twice that amount in width and in height.
- **Heavy effects**. Big blurs and many animated layers use CPU in OBS. A gradient looks the same as blur for glows and costs far less.
- **Blur in the gallery**. Blur filters can flicker in Firefox's gallery previews. They are fine in OBS.
- **Test backgrounds**. The gallery has color swatches (dark, black, white, green, checker) to check transparent themes.
- **Fonts**. Put font files in a fonts folder and load them with *@font-face* at the top of your theme.
- **Sharing**. A theme is one file, so you can share it by sending the *.css* file.

## Using themes from other people

A theme is a CSS file, and CSS can load images and fonts from anywhere on the internet.
A theme that does this makes the overlay contact the website every time it loads, which tells the website your IP address and when you're streaming. Before using a theme from someone else, open it in a text editor and look for `http://` or `https://`. Fonts and images in the theme's own folder, or in `static/fonts`, are fine.