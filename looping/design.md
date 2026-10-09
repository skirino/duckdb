# TODO
- Pick a file that is lexicographically the 1st of `looping/tasks/*.md`.
  The file contains a description of a task.
- If no file exists in `looping/tasks`, switch the state in `tmp/STATE.md` to `NO_TASK`
  and finish the execution.
- Read the task description. Before doing the actual task, plan what needs to be
  done. Consider for example the following points:
  - Functional requirements (what needs to be done)
  - Non-functional requirements (e.g. performance, security)
  - Acceptance criteria (e.g. if you are asked to add a new feature, what kind of tests
    are needed to cover the new feature)
- Write down the plan to `tmp/DESIGN.md`. The file is used by the following steps.
- When finished, switch the state in `tmp/STATE.md` to `DESIGNED`.
