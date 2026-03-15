### Bugfixes

- **History view hover fixes**: Fixed label hover not working on the day-grid heatmap canvas — hovering a cell containing a label now correctly highlights matching labels and shows the label tooltip. Fixed week view hover completely missing — added mouse event handlers to the day-dots canvas so both epoch dot and label circle hovers show tooltips. Moved tooltip rendering outside view-mode conditionals so tooltips are visible in all views.
