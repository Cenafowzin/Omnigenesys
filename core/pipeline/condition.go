package pipeline

// Condition avalia se uma célula (x, y) é elegível para uma operação.
// Todos os conditions de um operador precisam ser verdadeiros para a célula ser processada.
type Condition interface {
	Check(ctx *Context, x, y int) bool
}

// LayerIs passa quando a célula (x,y) no layer indicado for do tipo especificado.
type LayerIs struct {
	Layer string
	Type  string
}

func (c LayerIs) Check(ctx *Context, x, y int) bool {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return false
	}
	return layer.Cells[y][x].Type == c.Type
}

// LayerNot passa quando a célula (x,y) no layer indicado NÃO for do tipo especificado.
type LayerNot struct {
	Layer string
	Type  string
}

func (c LayerNot) Check(ctx *Context, x, y int) bool {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return true
	}
	return layer.Cells[y][x].Type != c.Type
}

// LayerEmpty passa quando a célula (x,y) no layer indicado estiver vazia (Type == "").
type LayerEmpty struct {
	Layer string
}

func (c LayerEmpty) Check(ctx *Context, x, y int) bool {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return true
	}
	return layer.Cells[y][x].Type == ""
}

// NotNearType passa quando não houver nenhuma célula do tipo especificado
// dentro do raio Distance (em células) no layer indicado.
type NotNearType struct {
	Layer    string
	Type     string
	Distance int
}

func (c NotNearType) Check(ctx *Context, x, y int) bool {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return true
	}
	for dy := -c.Distance; dy <= c.Distance; dy++ {
		for dx := -c.Distance; dx <= c.Distance; dx++ {
			nx, ny := x+dx, y+dy
			if nx < 0 || nx >= ctx.Grid.Width || ny < 0 || ny >= ctx.Grid.Height {
				continue
			}
			if layer.Cells[ny][nx].Type == c.Type {
				return false
			}
		}
	}
	return true
}

// CheckAll retorna true se todos os conditions passarem para (x, y).
func CheckAll(conditions []Condition, ctx *Context, x, y int) bool {
	for _, c := range conditions {
		if !c.Check(ctx, x, y) {
			return false
		}
	}
	return true
}
