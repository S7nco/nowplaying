CUSTOM THEMES
=============

Put your own theme files (.css) in this folder.
Files here are not tracked by git, so your themes stay private.

HOW TO MAKE ONE
1.  Copy a file from ../examples (default.css is a good start)
1a. Can create a new .css file within the custom folder also
2.  Rename it, e.g. mytheme.css. Use only letters, numbers, - and _ 
    in the name (no spaces or extra dots: "my theme.css" or "neon.v2.css" won't work).
    Also make sure Windows isn't hiding the extension and naming it mytheme.css.txt.
3.  Change the values inside.

HOW TO USE it
Open the overlay with the theme name in the URL:
    http://localhost:5000/overlay.html?theme=mytheme

To get it in the tray menu (Open in Browser / Copy URL),
star the theme via the Theme Gallery.

GOOD TO KNOW
-   If a custom theme has the same name as an example, the custom one is used.
-   The list of settings you can change is in ../examples/default.css
-   To use a font, put the font folder in static/fonts and point
    to it in an @font-face rule, for example:
    src: url('/fonts/MyFont/MyFont-Bold.ttf') format('truetype');
