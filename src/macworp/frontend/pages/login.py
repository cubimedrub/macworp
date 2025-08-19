from typing import Dict
import logging

from nicegui import ui, app

from macworp.configuration import Configuration
from macworp.frontend.components.login.credentials_form import create_credentials_form
from macworp.frontend.components.login.oauth_form import create_oauth_form
from macworp.frontend.components.login.provider_selection import (
    create_provider_card,
    create_provider_selection,
)
from macworp.frontend.services.login_service import LoginService

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)


class LoginPage:
    """Main login page component"""

    def __init__(self, config: Configuration):
        self.config = config
        self.login_service = LoginService(config)
        self.providers = {}
        self.selected_provider_type = None
        self.selected_provider = None
        self.username_input = None
        self.password_input = None
        self.login_button = None
        self.error_message = None
        self.loading_spinner = None
        self.provider_cards = None

        # Initialize containers
        self.form_container = None
        self.provider_container = None

    async def load_providers(self):
        """Load available login providers"""
        self.providers = await self.login_service.get_login_providers()
        logger.info(f"Loaded providers: {self.providers}")

    def select_provider(self, provider_type: str, provider: str):
        """Handle provider selection"""
        self.selected_provider_type = provider_type
        self.selected_provider = provider

        logger.info(f"Selected provider: {provider_type}/{provider}")

        if self.error_message:
            self.error_message.set_text("")

        self.show_login_form()

    def show_login_form(self):
        """Display the appropriate login form based on provider type"""
        if self.form_container:
            self.form_container.clear()
        else:
            with self.page_container:
                self.form_container = ui.column().classes(
                    "w-full max-w-md mx-auto space-y-4"
                )

        # Clear provider selection
        if self.provider_container:
            self.provider_container.clear()

        with self.form_container:
            # Provider info
            ui.label(f"Login with {self.selected_provider.title()}").classes(
                "text-xl font-bold text-center"
            )
            ui.label(f"Provider type: {self.selected_provider_type}").classes(
                "text-sm text-gray-600 text-center"
            )

            if self.selected_provider_type in ["database", "file"]:
                # Create and store credentials form references with fixed callback
                credentials_form = create_credentials_form(self.handle_credential_login)
                self.username_input = credentials_form["username_input"]
                self.password_input = credentials_form["password_input"]
                self.login_button = credentials_form["login_button"]
                self.loading_spinner = credentials_form["loading_spinner"]

            elif self.selected_provider_type == "openid":
                # Create OAuth form
                create_oauth_form(self.selected_provider, self.handle_oauth_login)

    async def handle_credential_login(self, username=None, password=None):
        """Handle username/password login"""

        # If called with parameters from the form callback, use those
        if username is not None and password is not None:
            username_value = username
            password_value = password
        else:
            # If called directly (e.g., from button click), get values from inputs
            username_value = self.username_input.value if self.username_input else None
            password_value = self.password_input.value if self.password_input else None

        if not username_value or not password_value:
            self.show_error("Please enter both username and password")
            return

        # Show loading state
        if self.login_button:
            self.login_button.set_text("Logging in...")
            self.login_button.disable()
        if self.loading_spinner:
            self.loading_spinner.set_visibility(True)

        try:
            result = await self.login_service.login_with_credentials(
                self.selected_provider_type,
                self.selected_provider,
                username_value,
                password_value,
            )

            if "error" in result:
                self.show_error(result["error"])
            else:
                # Login successful
                await self.handle_login_success(result)

        except Exception as e:
            self.show_error(f"Login failed: {str(e)}")
        finally:
            # Reset loading state
            if self.login_button:
                self.login_button.set_text("Login")
                self.login_button.enable()
            if self.loading_spinner:
                self.loading_spinner.set_visibility(False)

    async def handle_oauth_login(self):
        """Handle OAuth login initiation"""
        try:
            redirect_url = await self.login_service.initiate_oauth_login(
                self.selected_provider_type, self.selected_provider
            )

            if redirect_url:
                # Redirect to OAuth provider
                ui.navigate.to(redirect_url, new_tab=False)
            else:
                self.show_error("Failed to initiate OAuth login")

        except Exception as e:
            self.show_error(f"OAuth login failed: {str(e)}")

    async def handle_login_success(self, login_result: Dict):
        """Handle successful login"""

        # todo use secure storage
        app.storage.user["authenticated"] = True
        app.storage.user["auth-token"] = login_result.get("jwt")

        ui.navigate.to("/projects")

    def show_error(self, message: str):
        """Display error message"""
        ui.notify(message, type="negative")
        if hasattr(self, "error_label") and self.error_label:
            self.error_label.set_text(message)

    def show_provider_selection(self):
        """Reset to provider selection view"""
        self.selected_provider_type = None
        self.selected_provider = None

        # Clear form container
        if self.form_container:
            self.form_container.clear()
            self.form_container = None

        # Clear error message
        if hasattr(self, "error_label") and self.error_label:
            self.error_label.set_text("")

        # Recreate provider selection
        if self.page_container:
            with self.page_container:
                self.provider_container = ui.column().classes("w-full")
                with self.provider_container:
                    create_provider_selection(
                        providers=self.providers, on_select=self.select_provider
                    )

    async def show(self):
        """Display the main login page"""
        await self.load_providers()

        with ui.column().classes("w-full items-center"):
            with ui.card().classes("w-full max-w-md mx-auto p-8 text-center mb-6"):
                ui.label("NF Cloud Login").style(
                    "color: #6E93D6; font-size: 200%; font-weight: 300"
                )
                ui.label("Welcome back! Please sign in to continue.").classes(
                    "text-gray-600 mt-2"
                )

            with ui.column().classes(
                "text-center min-h-screen bg-white-50 py-12 px-4"
            ) as self.page_container:
                self.error_label = ui.label("").classes("text-red-500 text-center")
                if not self.selected_provider_type:
                    create_provider_selection(
                        providers=self.providers, on_select=self.select_provider
                    )
