# internal imports
from nf_cloud_backend.command_line_interface import ComandLineInterface

def main():
    """
    Main function
    """
    cli = ComandLineInterface()
    cli.start()

if __name__ == "__main__":
    main()
