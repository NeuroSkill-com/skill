### UI

- **History view UX improvements**: 14 enhancements to the history view:
  1. Week view shows total recording duration per day in sidebar (e.g. "2s · 1h 30m").
  2. Clicking on the day grid heatmap scrolls to and expands the corresponding session.
  3. Week view entire row is clickable to navigate to day view (not just sidebar).
  4. Month view calendar cells show mini duration bars proportional to recording hours.
  5. Day grid draws a red "now" marker hairline when viewing today.
  6. Keyboard shortcuts 1/2/3/4 switch between year/month/week/day views; arrow keys navigate in calendar views too.
  7. Week view today row has a visible left-edge accent bar instead of nearly invisible tint.
  8. Cross-highlighting between grid and session list — hovering a cell highlights the session row below; hovering grid sets a primary ring on the matching session card.
  9. Week↔Day view transitions use a subtle fade animation.
  10. Day view shows an aggregate daily summary card (total duration, avg relaxation/engagement, label count) above the session list when there are multiple sessions.
  11. Fixed `daySessionCounts` always returning 1 — heatmap intensity now reflects actual session count per day from cache.
  12. Month/year view tooltips show recording duration alongside session count.
  13. Expanding a session row scrolls it into view smoothly.
  14. Week view shows small session color legend dots in each day row.

### Bugfixes

- **Heatmap session counts**: `daySessionCounts` now reads from the localStorage day cache to return actual session counts instead of always 1, making month/year heatmap intensity meaningful.
