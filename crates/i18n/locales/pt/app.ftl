# Main window: top bar, tree canvas, ficha, list, notices.
app.cli.usage = uso: genealogy-rs [ARQUIVO.ged]\n\nExplore e edite árvores genealógicas GEDCOM.
app.menu.quit = Sair do gen
app.file.open_prompt = Abrir GEDCOM

app.topbar.people.one = {n} pessoa
app.topbar.people.other = {n} pessoas
app.topbar.families.one = {n} família
app.topbar.families.other = {n} famílias
app.topbar.saved = salvo
app.topbar.unsaved = não salvo
app.topbar.search = Buscar pessoa, lugar, ano
app.topbar.undo = Desfazer (⌘Z)
app.topbar.redo = Refazer (⌘⇧Z)
app.topbar.open = Abrir arquivo
app.topbar.export = Exportar GEDCOM
app.topbar.new_person = + Pessoa

app.tab.pedigree = Ascendência
app.tab.descendants = Descendentes
app.tab.fan = Leque
app.tab.list = Lista

app.person.no_dates = Sem datas

app.canvas.empty = Abra um arquivo GEDCOM (⌘O) ou comece uma árvore nova.
app.canvas.loading = Carregando…
app.canvas.generations = Gerações
app.canvas.add_father = + Adicionar pai
app.canvas.add_mother = + Adicionar mãe
app.canvas.union_with = com {name}
app.canvas.union_unknown = com cônjuge desconhecido

app.column.focus = Pessoa em foco
app.column.parents = Pais
app.column.grandparents = Avós
app.column.great_grandparents = Bisavós
app.column.great2_grandparents = Trisavós
app.column.children = Filhos
app.column.grandchildren = Netos
app.column.great_grandchildren = Bisnetos
app.column.great2_grandchildren = Trinetos
app.column.generation = Geração {n}

app.quick.parent = + pai
app.quick.mother = + mãe
app.quick.partner = + cônjuge
app.quick.child = + filho

app.rel.father = Pai
app.rel.mother = Mãe
app.rel.partner = Cônjuge
app.rel.son = Filho
app.rel.daughter = Filha

app.sex.male = Homem
app.sex.female = Mulher
app.sex.unknown = Desconhecido

app.sheet.nobody = Ninguém selecionado. Abra um arquivo (⌘O) ou busque alguém (⌘K).
app.sheet.lived.one = viveu {n} ano
app.sheet.lived.other = viveu {n} anos
app.sheet.edit = ✎ Editar
app.sheet.add_relative = Adicionar parente
app.sheet.view_gedcom = Ver GEDCOM
app.sheet.delete = Excluir
app.sheet.delete_confirm_hint = Excluir esta pessoa e seus vínculos?
app.sheet.delete_confirm = Excluir
app.sheet.cancel = Cancelar
app.sheet.complete = Ficha {n} % completa
app.sheet.complete_action = Completar
app.sheet.timeline = Linha do tempo
app.sheet.no_events = Nenhum evento registrado.
app.sheet.no_place = Local não registrado
app.sheet.add_partner = Adicionar cônjuge
app.sheet.parents = Pais
app.sheet.siblings = Irmãos ({n})
app.sheet.see_all = Ver todos
app.sheet.see_less = Ver menos
app.sheet.children = Filhos ({n})
app.sheet.add_child = + Adicionar filho ou filha

app.list.name = Nome
app.list.years = Anos
app.list.birth_place = Local de nascimento

app.raw.title = Registro GEDCOM · {id}

app.toast.invalid_file = Não é um arquivo GEDCOM válido: {arg}
app.toast.read_failed = Não foi possível ler {arg}
app.toast.unsaved_repeat = Há alterações não salvas: repita a ação para descartá-las
app.toast.saved = Salvo em {arg}
app.toast.save_failed = Não foi possível salvar: {arg}
app.toast.edit_failed = Não foi possível aplicar a alteração: {arg}
app.toast.undone = Desfeito
app.toast.redone = Refeito
app.toast.deleted = Pessoa excluída (⌘Z para desfazer)

app.error.not_found = o arquivo não existe
app.error.permission_denied = permissão negada
