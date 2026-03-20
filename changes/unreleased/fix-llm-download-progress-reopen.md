### Bugfixes

- **LLM download progress lost on window reopen**: When starting a model download in LLM settings, closing the window, and reopening it, the download progress bar was not shown. The poll timer only refreshed the catalog when it already knew about an active download, creating a chicken-and-egg problem on fresh mounts. Fixed by always polling the catalog (a cheap in-memory read) so in-flight downloads are detected regardless of initial component state. Also added the missing `"paused"` variant to the frontend `DownloadState` type.
