CUSTOM THEMES
=============

Put your own theme files (.css) in this folder.
Files here are not tracked by git, so your themes stay private.

HOW TO MAKE ONE
1.  Copy a file from ../examples (default.css is a good start)
1a. Can create a new .css file within the custom folder also
2.  Rename it to whatever you want, e.g. mytheme.css
    (Make sure Windows isn't hiding the extension and 
    naming it mytheme.css.txt)
3.  Change the values inside.

HOW TO USE it
Open the overlay with the theme name in the URL:
    http://localhost:5000/overlay.html?theme=mytheme

To get it in the tray menu (Open in Browser / Copy URL > Custom),
choose Restart from the tray menu. Editing a theme file that is 
already in the menu does not need a restart, just refresh the page. 

GOOD TO KNOW
-   If a custom theme has the same name as an example, the custom one is used.
-   The list of settings you can change is in ../examples/default.css
-   To use a font, put the font folder in static/fonts and point
    to it in an @font-face rule, for example:
    src: url('/fonts/MyFont/MyFont-Bold.ttf') format ('truetype');
