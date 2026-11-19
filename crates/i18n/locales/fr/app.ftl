# Main window: top bar, tree canvas, ficha, list, notices.
app.cli.usage = usage : genealogy-rs [FICHIER.ged]\n\nExplorez et modifiez des arbres généalogiques GEDCOM.
app.menu.quit = Quitter gen
app.file.open_prompt = Ouvrir un GEDCOM

app.topbar.people.one = {n} personne
app.topbar.people.other = {n} personnes
app.topbar.families.one = {n} famille
app.topbar.families.other = {n} familles
app.topbar.saved = enregistré
app.topbar.unsaved = non enregistré
app.topbar.search = Rechercher une personne, un lieu, une année
app.topbar.undo = Annuler (⌘Z)
app.topbar.redo = Rétablir (⌘⇧Z)
app.topbar.open = Ouvrir un fichier
app.topbar.export = Exporter en GEDCOM
app.topbar.new_person = + Personne

app.tab.pedigree = Ascendance
app.tab.descendants = Descendance
app.tab.fan = Éventail
app.tab.list = Liste

app.person.no_dates = Sans dates

app.canvas.empty = Ouvrez un fichier GEDCOM (⌘O) ou commencez un nouvel arbre.
app.canvas.loading = Chargement…
app.canvas.generations = Générations
app.canvas.add_father = + Ajouter le père
app.canvas.add_mother = + Ajouter la mère
app.canvas.union_with = avec {name}
app.canvas.union_unknown = avec un conjoint inconnu
app.canvas.add_child = + Ajouter un enfant
app.canvas.generations_more = Plus de générations (+)
app.canvas.generations_fewer = Moins de générations (−)
app.fan.clamped = L'éventail affiche jusqu'à {n} générations

app.column.focus = Personne au centre
app.column.parents = Parents
app.column.grandparents = Grands-parents
app.column.great_grandparents = Arrière-grands-parents
app.column.great2_grandparents = Arrière-arrière-grands-parents
app.column.children = Enfants
app.column.grandchildren = Petits-enfants
app.column.great_grandchildren = Arrière-petits-enfants
app.column.great2_grandchildren = Arrière-arrière-petits-enfants
app.column.generation = Génération {n}

app.quick.parent = + père
app.quick.mother = + mère
app.quick.partner = + conjoint
app.quick.child = + enfant

app.rel.father = Père
app.rel.mother = Mère
app.rel.partner = Conjoint
app.rel.son = Fils
app.rel.daughter = Fille

app.sex.male = Homme
app.sex.female = Femme
app.sex.unknown = Inconnu

app.sheet.nobody = Personne n'est sélectionné. Ouvrez un fichier (⌘O) ou cherchez quelqu'un (⌘K).
app.sheet.lived.one = a vécu {n} an
app.sheet.lived.other = a vécu {n} ans
app.sheet.edit = ✎ Modifier
app.sheet.add_relative = Ajouter un proche
app.sheet.view_gedcom = Voir le GEDCOM
app.sheet.merge = Fusionner avec une autre personne…
app.sheet.kinship = Calculer la parenté avec…
app.sheet.delete = Supprimer
app.sheet.delete_confirm_hint = Supprimer cette personne et ses liens ?
app.sheet.delete_confirm = Supprimer
app.sheet.cancel = Annuler
app.sheet.complete = Fiche complète à {n} %
app.sheet.complete_action = Compléter
app.sheet.timeline = Chronologie
app.sheet.no_events = Aucun événement enregistré.
app.sheet.no_place = Lieu non renseigné
app.sheet.add_partner = Ajouter un conjoint
app.sheet.parents = Parents
app.sheet.siblings = Frères et sœurs ({n})
app.sheet.see_all = Tout voir
app.sheet.see_less = Voir moins
app.sheet.children = Enfants ({n})
app.sheet.add_child = + Ajouter un fils ou une fille

app.list.name = Nom
app.list.years = Années
app.list.birth_place = Lieu de naissance

app.raw.title = Enregistrement GEDCOM · {id}

app.toast.invalid_file = Ce n'est pas un fichier GEDCOM valide : {arg}
app.toast.read_failed = Impossible de lire {arg}
app.toast.unsaved_repeat = Modifications non enregistrées : répétez l'action pour les abandonner
app.toast.saved = Enregistré dans {arg}
app.toast.save_failed = Impossible d'enregistrer : {arg}
app.toast.edit_failed = Impossible d'appliquer la modification : {arg}
app.toast.undone = Annulé
app.toast.redone = Rétabli
app.toast.deleted = Personne supprimée (⌘Z pour annuler)
app.toast.merged = Personnes fusionnées
app.toast.undo = Annuler

app.error.not_found = le fichier n'existe pas
app.error.permission_denied = permission refusée

app.topbar.review = Révision des données (⌘⇧R)
app.sheet.review_hint.one = {n} problème dans la révision
app.sheet.review_hint.other = {n} problèmes dans la révision
app.review.title = Révision
app.review.filter.all = Tous · {n}
app.review.filter.errors = Erreurs · {n}
app.review.filter.warnings = Avertissements · {n}
app.review.filter.info = Infos · {n}
app.review.kind = {kind} · {n}
app.review.family = Famille {id}
app.review.edit = Modifier
app.review.merge = Fusionner
app.review.ignore = Ignorer
app.review.more.one = Voir {n} de plus
app.review.more.other = Voir {n} de plus
app.review.loading = Révision de l'arbre…
app.review.empty = Aucun problème trouvé.
app.review.empty_filter = Rien dans cette catégorie.
app.review.footer.focus = centrer
app.review.footer.close = fermer
