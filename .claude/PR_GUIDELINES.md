# Pull Request Guidelines

When creating pull requests for this repository, always include detailed update notes so reviewers can quickly understand what changed.

## PR Description Format

Use this structure for all PR descriptions:

```markdown
## Summary
Brief 1-2 sentence overview of what this PR accomplishes.

## Changes
- **[Category]**: Description of change
- **[Category]**: Description of change
- ...

## Technical Details
(Optional) Any implementation notes, architectural decisions, or important context.

## Testing
How to verify the changes work:
1. Step one
2. Step two
3. ...
```

## Categories to Use

- **Added**: New features or files
- **Changed**: Modifications to existing functionality
- **Fixed**: Bug fixes
- **Removed**: Deleted code or features
- **Updated**: Dependency updates, version bumps
- **Refactored**: Code restructuring without behavior change
- **Docs**: Documentation changes

## Example

```markdown
## Summary
Add automatic dependency installation to the one-click launcher.

## Changes
- **Added**: Automatic Node.js installation via winget or direct MSI download
- **Added**: Automatic Rust installation via winget or rustup-init.exe
- **Changed**: Replace localized 'pause' text with custom English messages
- **Added**: User choice menu: [1] Automatic [2] Manual [3] Cancel

## Technical Details
- Uses winget as primary installation method (Windows 10/11)
- Falls back to direct download if winget unavailable
- Rust installer runs with `-y` flag for silent default installation
- Requires restart after Rust install due to PATH updates

## Testing
1. Clone repo on Windows machine without Rust installed
2. Double-click START_HIVE.bat
3. Select option [1] for automatic install when prompted
4. Verify Rust downloads and installs
5. Restart and run script again to continue build
```

## Commit Messages

Follow conventional commit style:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `refactor:` Code restructuring
- `chore:` Maintenance tasks

Keep the first line under 72 characters, add details in the body if needed.
