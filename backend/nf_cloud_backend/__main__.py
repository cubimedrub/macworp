from pathlib import Path
import sys

from sqlmodel import Session

from .database import engine, seed
from .tests.runner import run_tests

def main():
    command = sys.argv[1]

    match command:
        case "seed":
            cli_seed(Path(sys.argv[2]))
        case "test":
            cli_test(Path(sys.argv[2]) if len(sys.argv) > 2 else None)

def cli_seed(path: Path):
    with Session(engine) as session:
        seed(session, path, True)
        session.commit()

def cli_test(output: Path | None):
    run_tests(output)

if __name__ == "__main__":
    main()