### UI

- **History day view — 24×720 heatmap grid**: replaced the linear 24-hour timeline bar and epoch dot canvas with a dense heatmap grid (24 hour-columns × 720 five-second rows); cells colored by session color with opacity modulated by relaxation+engagement; hour headers, 15-minute grid lines, minute labels; scrollable canvas (max 420px); cursor-following tooltip with HH:MM:SS and data values.

- **History day view — rainbow label circles**: replaced text-based label display with tiny colored circles in session rows, expanded details, timeline legend, and canvas; rainbow hue distribution (0°–300° HSL) based on temporal order; hover highlights exact-match labels (glow ring + scale) and temporally close labels (within 5 min, brightness/glow); popover tooltips on hover; cross-session matching.

- **History day view — screenshot indicators on heatmap**: cells that have a corresponding screenshot show a small blue diamond indicator on the canvas grid; hovering a screenshot cell displays a floating image preview (loaded from the local API server); preview disappears immediately when the mouse moves to a different cell; screenshot data loaded per-day via `get_screenshots_around`.
