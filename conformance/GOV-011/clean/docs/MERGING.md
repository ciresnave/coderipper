# Reading a conflict

Git leaves this in a file:

```text
<<<<<<< HEAD
run the device tests with --features gpu
=======
run the device tests with --features cuda
>>>>>>> feature/cuda
```

Resolve it, then commit.
