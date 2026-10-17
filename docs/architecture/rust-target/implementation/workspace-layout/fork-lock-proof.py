"""Standalone synthetic POSIX proof: CLOEXEC is not a close-on-fork guarantee."""
import fcntl
import os
import tempfile

with tempfile.TemporaryDirectory(prefix="rm-layout-fork-") as root:
    held = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
    read_end, write_end = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(write_end)
        os.read(read_end, 1)
        os._exit(0)
    os.close(read_end)
    os.close(held)
    contender = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        try:
            fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("Inherited CLOEXEC descriptor retains flock before child exec/exit")
        else:
            raise AssertionError("inherited child must still own lock")
    finally:
        os.write(write_end, b"x")
        os.close(write_end)
        _, status = os.waitpid(child, 0)
        assert os.waitstatus_to_exitcode(status) == 0
    fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
    os.close(contender)
    print("Acquired after child exit and explicit reap")
