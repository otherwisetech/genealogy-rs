# Main window: top bar, tree canvas, ficha, list, notices.
app.cli.usage = usage: gen [FILE.ged]\n\nExplore and edit GEDCOM family trees.
app.menu.quit = Quit gen
app.file.open_prompt = Open GEDCOM

app.topbar.people.one = {n} person
app.topbar.people.other = {n} people
app.topbar.families.one = {n} family
app.topbar.families.other = {n} families
app.topbar.saved = saved
app.topbar.unsaved = unsaved
app.topbar.search = Search person, place, year
app.topbar.undo = Undo (⌘Z)
app.topbar.redo = Redo (⌘⇧Z)
app.topbar.open = Open file
app.topbar.export = Export GEDCOM
app.topbar.new_person = + Person

app.tab.pedigree = Pedigree
app.tab.descendants = Descendants
app.tab.fan = Fan
app.tab.list = List

app.person.no_dates = No dates

app.canvas.empty = Open a GEDCOM file (⌘O) or start a new tree.
app.canvas.loading = Loading…
app.canvas.generations = Generations
app.canvas.add_father = + Add father
app.canvas.add_mother = + Add mother
app.canvas.union_with = with {name}
app.canvas.union_unknown = with unknown partner

app.column.focus = Focus person
app.column.parents = Parents
app.column.grandparents = Grandparents
app.column.great_grandparents = Great-grandparents
app.column.great2_grandparents = 2× great-grandparents
app.column.children = Children
app.column.grandchildren = Grandchildren
app.column.great_grandchildren = Great-grandchildren
app.column.great2_grandchildren = 2× great-grandchildren
app.column.generation = Generation {n}

app.quick.parent = + parent
app.quick.mother = + mother
app.quick.partner = + partner
app.quick.child = + child

app.rel.father = Father
app.rel.mother = Mother
app.rel.partner = Partner
app.rel.son = Son
app.rel.daughter = Daughter

app.sex.male = Male
app.sex.female = Female
app.sex.unknown = Unknown

app.sheet.nobody = Nobody selected. Open a file (⌘O) or search for someone (⌘K).
app.sheet.lived.one = lived {n} year
app.sheet.lived.other = lived {n} years
app.sheet.edit = ✎ Edit
app.sheet.add_relative = Add relative
app.sheet.view_gedcom = View GEDCOM
app.sheet.delete = Delete
app.sheet.delete_confirm_hint = Delete this person and their links?
app.sheet.delete_confirm = Delete
app.sheet.cancel = Cancel
app.sheet.complete = Profile {n} % complete
app.sheet.complete_action = Complete
app.sheet.timeline = Timeline
app.sheet.no_events = No events recorded.
app.sheet.no_place = Place not recorded
app.sheet.add_partner = Add partner
app.sheet.parents = Parents
app.sheet.siblings = Siblings ({n})
app.sheet.see_all = See all
app.sheet.see_less = See less
app.sheet.children = Children ({n})
app.sheet.add_child = + Add son or daughter

app.list.name = Name
app.list.years = Years
app.list.birth_place = Birth place


app.raw.title = GEDCOM record · {id}

app.toast.invalid_file = Not a valid GEDCOM file: {arg}
app.toast.read_failed = Could not read {arg}
app.toast.unsaved_repeat = Unsaved changes: repeat the action to discard them
app.toast.saved = Saved to {arg}
app.toast.save_failed = Could not save: {arg}
app.toast.edit_failed = Could not apply the change: {arg}
app.toast.undone = Undone
app.toast.redone = Redone
app.toast.deleted = Person deleted (⌘Z to undo)

app.error.not_found = the file does not exist
app.error.permission_denied = permission denied
