from nf_cloud_backend.auth.password_handler import Hasher
print(Hasher.get_password_hash("password"))