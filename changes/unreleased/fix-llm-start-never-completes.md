### Bugfixes

- **LLM server start never transitions from "loading"**: Fixed a bug where clicking "Start" in the chat window would show "loading" indefinitely. The root causes were: (1) the status poll timer self-cancelled on mount when status was "stopped" and was never restarted when the user clicked Start, (2) failed init paths (`init()` returning `None`, actor early-exit on model load or context creation failure) did not emit an `llm:status` event so the UI was never notified. Now the poll timer is restarted on every start attempt, and all failure paths emit a `"stopped"` status event with an error message.

### UI

- **Show LLM start errors in chat window**: When the LLM server fails to start, a dismissible red error banner now appears below the chat header with the failure reason (e.g. "no model selected", "model file not found", "failed to create context"). Previously the error was only logged to the browser console.
