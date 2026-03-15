### Bugfixes

- **History label dot tooltips no longer clipped**: Replaced `absolute`-positioned tooltips on label dots with a `fixed`-position portal element, preventing overflow clipping from parent containers with `overflow-hidden`. Tooltips now always render above all content regardless of DOM nesting.
