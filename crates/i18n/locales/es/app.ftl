# Main window: top bar, tree canvas, ficha, list, notices.
app.cli.usage = uso: gen [ARCHIVO.ged]\n\nExplora y edita árboles genealógicos GEDCOM.
app.menu.quit = Salir de gen
app.file.open_prompt = Abrir GEDCOM

app.topbar.people.one = {n} persona
app.topbar.people.other = {n} personas
app.topbar.families.one = {n} familia
app.topbar.families.other = {n} familias
app.topbar.saved = guardado
app.topbar.unsaved = sin guardar
app.topbar.search = Buscar persona, lugar, año
app.topbar.undo = Deshacer (⌘Z)
app.topbar.redo = Rehacer (⌘⇧Z)
app.topbar.open = Abrir archivo
app.topbar.export = Exportar GEDCOM
app.topbar.new_person = + Persona

app.tab.pedigree = Pedigrí
app.tab.descendants = Descendientes
app.tab.fan = Abanico
app.tab.list = Lista

app.person.no_dates = Sin fechas

app.canvas.empty = Abre un archivo GEDCOM (⌘O) o empieza un árbol nuevo.
app.canvas.loading = Cargando…
app.canvas.generations = Generaciones
app.canvas.add_father = + Añadir padre
app.canvas.add_mother = + Añadir madre
app.canvas.union_with = con {name}
app.canvas.union_unknown = con pareja desconocida

app.column.focus = Persona en foco
app.column.parents = Padres
app.column.grandparents = Abuelos
app.column.great_grandparents = Bisabuelos
app.column.great2_grandparents = Tatarabuelos
app.column.children = Hijos
app.column.grandchildren = Nietos
app.column.great_grandchildren = Bisnietos
app.column.great2_grandchildren = Tataranietos
app.column.generation = Generación {n}

app.quick.parent = + padre
app.quick.mother = + madre
app.quick.partner = + pareja
app.quick.child = + hijo

app.rel.father = Padre
app.rel.mother = Madre
app.rel.partner = Pareja
app.rel.son = Hijo
app.rel.daughter = Hija

app.sex.male = Hombre
app.sex.female = Mujer
app.sex.unknown = Desconocido

app.sheet.nobody = Nadie seleccionado. Abre un archivo (⌘O) o busca a alguien (⌘K).
app.sheet.lived.one = vivió {n} año
app.sheet.lived.other = vivió {n} años
app.sheet.edit = ✎ Editar
app.sheet.add_relative = Añadir pariente
app.sheet.view_gedcom = Ver GEDCOM
app.sheet.delete = Eliminar
app.sheet.delete_confirm_hint = ¿Eliminar a esta persona y sus vínculos?
app.sheet.delete_confirm = Eliminar
app.sheet.cancel = Cancelar
app.sheet.complete = Ficha completa al {n} %
app.sheet.complete_action = Completar
app.sheet.timeline = Línea de vida
app.sheet.no_events = Sin eventos registrados.
app.sheet.no_place = Lugar sin registrar
app.sheet.add_partner = Añadir pareja
app.sheet.parents = Padres
app.sheet.siblings = Hermanos ({n})
app.sheet.see_all = Ver todos
app.sheet.see_less = Ver menos
app.sheet.children = Hijos ({n})
app.sheet.add_child = + Añadir hijo o hija

app.list.name = Nombre
app.list.years = Años
app.list.birth_place = Lugar de nacimiento


app.raw.title = Registro GEDCOM · {id}

app.toast.invalid_file = No es un archivo GEDCOM válido: {arg}
app.toast.read_failed = No se pudo leer {arg}
app.toast.unsaved_repeat = Hay cambios sin guardar: repite la acción para descartarlos
app.toast.saved = Guardado en {arg}
app.toast.save_failed = No se pudo guardar: {arg}
app.toast.edit_failed = No se pudo aplicar el cambio: {arg}
app.toast.undone = Deshecho
app.toast.redone = Rehecho
app.toast.deleted = Persona eliminada (⌘Z para deshacer)

app.error.not_found = el archivo no existe
app.error.permission_denied = permiso denegado
