package pipeline

type Pipeline struct {
	Steps []Operator
}

func NewPipeline() *Pipeline {
	return &Pipeline{
		Steps: []Operator{},
	}
}

func (pipeline *Pipeline) AddStep(operator Operator) *Pipeline {
	pipeline.Steps = append(pipeline.Steps, operator)
	return pipeline
}

func (pipeline *Pipeline) Run(ctx *Context) error {
	for _, step := range pipeline.Steps {
		if err := step.Execute(ctx); err != nil {
			return err
		}
	}

	return nil
}
