# Français. Les textes manquants retombent sur l'anglais.

## Common

common-connecting = connexion...
common-server-unreachable = serveur injoignable ({ $error })
common-version = version { $version }
common-not-found = Cette page n'existe pas.
common-back-home = Retour à tes tableaux


## Login and accounts

auth-login-heading = Connexion
auth-login-field-login = E-mail ou nom d'utilisateur
auth-login-submit = Se connecter
auth-logout = Se déconnecter
auth-field-email = E-mail
auth-field-username = Nom d'utilisateur
auth-field-name = Nom
auth-field-password = Mot de passe

## First-launch setup

setup-heading = Configurer laneway
setup-intro = Crée le compte administrateur. Le code de configuration à usage unique se trouve dans les logs du serveur.
setup-field-token = Code de configuration
setup-submit = Créer le compte administrateur

## Boards

board-list-heading = Tableaux
board-list-empty = Aucun tableau pour l'instant.
board-new-name = Nom du nouveau tableau
board-new-submit = Créer le tableau
board-default-column-todo = À faire
board-default-column-doing = En cours
board-default-column-done = Terminé
board-rename = Renommer le tableau
board-not-found = Ce tableau n'existe pas ou tu n'y as pas accès.
column-new-name = Ajouter une colonne
column-rename = Renommer la colonne
column-delete = Supprimer la colonne (seulement si vide)
card-new-title = Ajouter une carte
card-edit-heading = Modifier la carte
card-field-title = Titre
card-field-description = Description
card-field-column = Colonne
card-delete = Supprimer la carte
column-settings = Paramètres de la colonne
column-drag = Déplacer la colonne
column-field-name = Nom
column-field-wip-limit = Limite WIP
column-field-wip-limit-hint = Laisser vide pour aucune limite.
column-field-done = Colonne terminée
column-field-done-hint = Les cartes de cette colonne comptent comme terminées.
column-over-limit = Plus de cartes que la limite WIP ne le permet
card-done = Terminée
card-field-priority = Priorité
card-priority-none = Aucune priorité
card-priority-low = Basse
card-priority-medium = Moyenne
card-priority-high = Haute
card-priority-urgent = Urgente
card-field-start = Début
card-field-due = Échéance
card-field-estimate = Estimation
card-field-estimate-hint = Points ou heures, selon ce que ton tableau utilise.
card-description-write = Écrire
card-description-preview = Aperçu
card-description-empty = Pas de description.
card-description-help = Le Markdown fonctionne : **gras**, _italique_, listes, - [ ] cases à cocher, tableaux, liens.
card-has-description = A une description
card-due-on = Échéance le { $date }
card-overdue = En retard depuis le { $date }
card-estimate-badge = Estimation : { $estimate }
label-manage = Étiquettes
label-manage-heading = Étiquettes du tableau
label-none = Pas encore d'étiquettes.
label-new-name = Nom de la nouvelle étiquette
label-new-submit = Ajouter l'étiquette
label-field-name = Nom de l'étiquette
label-field-color = Couleur
label-delete = Supprimer l'étiquette { $name }
card-field-labels = Étiquettes
label-color-gray = Gris
label-color-red = Rouge
label-color-orange = Orange
label-color-yellow = Jaune
label-color-green = Vert
label-color-teal = Sarcelle
label-color-blue = Bleu
label-color-purple = Violet
label-color-pink = Rose
dnd-instructions = Pour prendre une carte ou une colonne, appuie sur espace. Déplace-la avec les flèches, dépose-la avec espace ou entrée, annule avec échap.
dnd-picked-up = { $item } prise.
dnd-over = { $item } est au-dessus de { $target }.
dnd-dropped = { $item } déposée sur { $target }.
dnd-dropped-nowhere = { $item } déposée.
dnd-cancelled = Déplacement de { $item } annulé.
common-save = Enregistrer
common-cancel = Annuler

## Errors

error-unknown = Une erreur s'est produite. Réessaie.
error-invalid-credentials = E-mail, nom d'utilisateur ou mot de passe incorrect.
error-invalid-setup-token = Le code de configuration est faux ou a déjà été utilisé. Redémarrer laneway en génère un nouveau.
error-setup-done = La configuration est déjà terminée.
error-email-taken = Cet e-mail est déjà utilisé.
error-username-taken = Ce nom d'utilisateur est déjà pris.
error-invalid-email = Cela ne ressemble pas à une adresse e-mail.
error-invalid-username = Les noms d'utilisateur ont 2 à 32 caractères : lettres, chiffres, _ . et -, en commençant par une lettre ou un chiffre.
error-invalid-name = Le nom doit faire 1 à 100 caractères.
error-password-too-short = Le mot de passe doit faire au moins 10 caractères.
error-password-too-long = Le mot de passe peut faire au plus 512 caractères.
error-too-many-attempts = Trop de connexions échouées. Réessaie dans 15 minutes.
error-invalid-board-name = Les noms de tableau font 1 à 200 caractères.
error-invalid-column-name = Les noms de colonne font 1 à 200 caractères.
error-invalid-card-title = Les titres de carte font 1 à 500 caractères.
error-description-too-long = La description est trop longue.
error-invalid-wip-limit = La limite WIP doit être d'au moins 1.
error-column-not-empty = Déplace ou supprime d'abord les cartes de cette colonne.
error-not-found = Cela n'existe plus. Recharge la page.
error-invalid-priority = Choisis une des priorités.
error-invalid-dates = Le début ne peut pas être après l'échéance.
error-invalid-estimate = L'estimation doit être entre 0 et 100000.
error-invalid-label-name = Les noms d'étiquette font 1 à 50 caractères.
error-invalid-label-color = Choisis une des couleurs.
error-label-taken = Ce tableau a déjà une étiquette avec ce nom.
