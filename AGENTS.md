# AI Coding Assistant Rules

This document outlines rules for the AI assistant to follow when editing this codebase.

1.  **Prototype Mindset**: This is prototype code. Prioritize functionality and rapid iteration over production-ready, distributable code.
2.  **Single User Focus**: The primary and only user is the current developer. Design and implement features accordingly.
3.  **No Hypothetical Future-Proofing for Multiple Users**: Do not implement solutions for problems that would only arise with multiple users. Focus on the immediate needs of the single user.
4.  **Direct and Elegant Solutions**: This is a prototype, not precious code. Aim for the most direct yet elegant way to achieve the desired outcome. Simplicity and clarity are valued.
5.  **Purposeful Comments**: Comments should only be used to explain complex logic, tricky parts of the code, inputs/outputs, or interface contracts. Do not add comments that merely state what the code is doing (e.g., "removed this", "added that").
6.  **No Change Tracking Comments**: Don't add comments everywhere showing what you changed. Make changes directly.
7.  **Test Dependency Awareness**: When making edits, consider how many tests may depend on the code before making changes.
8.  **Debugging Mindset**: Never attribute problems to compiler bugs or memory corruption issues.
9.  **No Environmental Blame**: Never conclude that an issue is caused by an external or environmental factor.
10. **Fail Fast**: No quiet fallbacks. Break and panic to make issues obvious and easier to diagnose.
11. **Fix Underlying Issues**: Always address the root cause of a problem. Do not implement surface-level fixes that mask broken assumptions or underlying bugs.
12. **Workspace Command Limitation**: Never run --workspace commands as the codebase is large.
13. **CLI Profiling Preference**: Use CLI and text-based profiling tools only. Do not suggest flamegraph or web-based profiling tools.
14. **Ground Truth Identification**: Before coding, be confident about what is ground truth and what is being edited. Markdown files, especially read-only ones, are likely ground truth.
15. **Incremental Progress**: Fix errors in accessible files even if you can't fix everything. Make progress where possible rather than stopping because some files are missing or inaccessible.
16. **Helper Functions & Clean Flow**: Break complex tasks into small, purpose-specific helper functions and keep your main function logic clear and linear so the overall control flow is easy to understand.

**NEVER MODIFY ANYTHING IN the vendor directory** That is third party code that we do not change.
