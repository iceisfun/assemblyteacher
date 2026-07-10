# Entry Point to main

This lesson explains why the executable entry point is not usually `main`: the
loader transfers control to a runtime startup stub, the runtime prepares process
state, runs initializers, and only then calls the source-level `main` or Windows
entry function.
