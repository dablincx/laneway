# English, the default language. Every other file falls back to this one.
# Ids are prefixed with their area (common-, auth-, setup-, error-, ...).

## Common

common-app-name = laneway
common-connecting = connecting...
common-server-unreachable = server not reachable ({ $error })
common-version = version { $version }

## Home

home-welcome = Hi { $name }

## Login and accounts

auth-login-heading = Log in
auth-login-field-login = Email or username
auth-login-submit = Log in
auth-logout = Log out
auth-field-email = Email
auth-field-username = Username
auth-field-name = Name
auth-field-password = Password

## First-launch setup

setup-heading = Set up laneway
setup-intro = Create the admin account. The one-time setup token is printed in the server logs.
setup-field-token = Setup token
setup-submit = Create admin account

## Errors

error-unknown = Something went wrong. Try again.
error-invalid-credentials = Wrong email, username or password.
error-invalid-setup-token = The setup token is wrong or was already used. Restarting laneway prints a new one.
error-setup-done = Setup is already done.
error-email-taken = That email is already used.
error-username-taken = That username is already taken.
error-invalid-email = That doesn't look like an email address.
error-invalid-username = Usernames are 2 to 32 characters: letters, digits, _ . and -, starting with a letter or digit.
error-invalid-name = The name must be 1 to 100 characters.
error-password-too-short = The password needs at least 10 characters.
error-password-too-long = The password can have at most 512 characters.
error-too-many-attempts = Too many failed logins. Try again in 15 minutes.
