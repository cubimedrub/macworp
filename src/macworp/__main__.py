from macworp.cli.command_line_interface import CommandLineInterface


def main():
    cli = CommandLineInterface()
    cli.args.func(cli.args)


if __name__ in {"__main__", "__mp_main__"}:
    main()
