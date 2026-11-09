# English, the default language. Every other file falls back to this one.
# Ids are prefixed with their area (common-, auth-, setup-, error-, ...).

## Common

common-app-name = laneway
common-connecting = connecting...
common-server-unreachable = server not reachable ({ $error })
common-version = version { $version }


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

## Boards

board-list-heading = Boards
board-list-empty = No boards yet.
board-new-name = New board name
board-new-submit = Create board
board-default-column-todo = To do
board-default-column-doing = Doing
board-default-column-done = Done
board-rename = Rename board
board-not-found = This board doesn't exist or you don't have access to it.
column-new-name = Add a column
column-rename = Rename column
column-move-left = Move column left
column-move-right = Move column right
column-delete = Delete column (only when empty)
card-new-title = Add a card
card-edit-heading = Edit card
card-field-title = Title
card-field-description = Description
card-field-column = Column
card-delete = Delete card
common-save = Save
common-cancel = Cancel

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
error-invalid-board-name = Board names are 1 to 200 characters.
error-invalid-column-name = Column names are 1 to 200 characters.
error-invalid-card-title = Card titles are 1 to 500 characters.
error-description-too-long = The description is too long.
error-invalid-wip-limit = The WIP limit must be at least 1.
error-column-not-empty = Move or delete the cards in this column first.
error-not-found = That doesn't exist anymore. Reload the page.
