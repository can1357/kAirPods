.pragma library

// Depth shading for the widget's layered shadows and backdrop. The design stacks
// translucent black layers, which reads as depth on dark themes but turns light
// themes grey, so light backgrounds get a much fainter shade.

/** True when `background` is a dark color (Rec. 601 luma below 0.5). */
function isDark(background) {
    return 0.299 * background.r + 0.587 * background.g + 0.114 * background.b < 0.5
}

/** Black at `alpha` on dark themes, at a quarter of it on light themes. */
function shadow(alpha, background) {
    return Qt.rgba(0, 0, 0, isDark(background) ? alpha : alpha / 4)
}
