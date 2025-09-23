backend: python -m macworp backend:start
# In development mode, NiceGUIs replace-feature is active which does not play along well when called with `python -m macworp`
# hence we call main directly 
frontend: python src/macworp/__main__.py frontend:start
worker: python -m macworp worker:start
