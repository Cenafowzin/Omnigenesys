package pipeline

type Operator interface {
	Execute(ctx *Context) error
}
