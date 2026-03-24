### UI

- **LLM loading progress steps in chat window**: The chat window now shows a step-by-step progress indicator while the LLM server is starting, replacing the generic bouncing dots. Each phase is shown with a spinner (active), checkmark (done), or dot (pending): Loading model weights → Creating context → Loading vision projector → Compiling GPU shaders. A progress bar tracks overall advancement. The current loading phase is also shown in the chat header (visible even when messages exist from a previous conversation). All steps are fully localised (en, de, fr, he, uk).
