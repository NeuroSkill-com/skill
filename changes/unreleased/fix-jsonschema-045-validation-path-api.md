### Bugfixes

- **jsonschema 0.45 compatibility in tool argument validation**: updated `skill-tools` validation error path extraction to use `ValidationError::instance_path()` so the workspace compiles and validation errors still report schema paths correctly after the dependency upgrade.
