# Lab: Patch the Check

A hands-on reverse-engineering lab: read a keycheck, find the single branch that
decides success or failure, and defeat it by inverting that branch — without
knowing the key. Teaches why "compare a secret and branch" protects nothing, and
what real software does instead (make the check's result decrypt or verify, so a
patched branch yields garbage). An emulate exercise where the student patches the
branch and the program grants access with the wrong key still loaded; every
reference solution runs on the emulator.
