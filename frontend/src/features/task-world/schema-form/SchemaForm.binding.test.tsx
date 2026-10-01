import {describe,it,expect} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import SchemaForm from './SchemaForm';

describe('typed input sources in schema form',()=>{
  it('does not require a literal value when an upstream binding supplies the field',()=>{
    const props={schema:{type:'object',properties:{value:{type:'string'}},required:['value']},value:'{}',onChange:()=>{},
      isBound:(path:string[])=>path[0]==='value',inputSource:()=> <span>由上游节点提供</span>};
    const html=renderToStaticMarkup(<SchemaForm {...props}/>);
    expect(html).toContain('由上游节点提供');
    expect(html).not.toContain('required=""');
  });
  it('keeps literal-only required fields and input labels compatible',()=>{
    const html=renderToStaticMarkup(<SchemaForm schema={{type:'object',properties:{value:{type:'string'}},required:['value']}} value="{}" onChange={()=>{}}/>);
    expect(html).toContain('required=""');expect(html).toContain('value *');
  });
});
