"""Shared checkbox styles and behavior in native WebKitGTK."""
import base64


def check(client, native, directory, diff, mark):
    # Read styles from both actual application controls, including their indicator.
    def probe():
        return client.js("""
            return ['.diff-uncommitted-field input','.diff-viewed input'].map(selector=>{
              const el=document.querySelector(selector), style=getComputedStyle(el), mark=getComputedStyle(el,'::after'), r=el.getBoundingClientRect();
              return {checked:el.checked,shared:el.classList.contains('ui-checkbox'),width:r.width,height:r.height,border:style.borderTopWidth,borderColor:style.borderTopColor,background:style.backgroundColor,opacity:style.opacity,indicator:mark.visibility,shadow:style.boxShadow};
            });
        """)
    native.wait_for(lambda:client.js("return document.querySelector('.diff-viewed input') && !document.querySelector('.diff-spinner')"),'loaded checkboxes')
    client.click('.diff-uncommitted-field')
    controls=probe()
    assert all(control['shared'] and control['width']==16 and control['height']==16 and control['border']=='1px' and control['indicator']=='hidden' and control['background']!='rgba(0, 0, 0, 0)' and control['borderColor']!=control['background'] for control in controls),controls
    assert controls[0]['background']==controls[1]['background'],controls
    client.keys(' ')
    native.wait_for(lambda:probe()[0]['checked'],'Space toggles comparison checkbox')
    assert client.js("return document.querySelector('.diff-uncommitted-field input').matches(':focus-visible') && getComputedStyle(document.querySelector('.diff-uncommitted-field input')).boxShadow!=='none'")
    client.click('.diff-viewed')
    native.wait_for(lambda:probe()[1]['checked'],'Viewed label toggles checkbox')
    assert client.js("return document.querySelector('.diff-fold').getAttribute('aria-expanded')==='false'")
    controls=probe()
    assert all(control['indicator']=='visible' for control in controls) and controls[0]['background']==controls[1]['background'],controls
    client.keys(' ')
    native.wait_for(lambda:not probe()[1]['checked'],'Space unmarks viewed')
    assert client.js("return document.querySelector('.diff-fold').getAttribute('aria-expanded')==='true'")
    client.js("const input=document.querySelector('.diff-viewed input');input.disabled=true;input.click()")
    assert probe()[1]['checked'] is False and probe()[1]['opacity']=='0.45'
    client.js("document.querySelector('.diff-viewed input').disabled=false")
    mark('Shared 16px checkboxes show unchecked borders and checked indicators; labels and Space update comparison/review behavior; keyboard focus is visible; disabled controls do not toggle')
    (directory/'checkboxes-dark.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
    def change_theme(theme):
        previous=probe()[0]['background']
        client.click('.titlebar-menu > summary')
        client.click("//button[normalize-space()='Appearance…']",'xpath')
        client.js("const select=document.querySelector('dialog select');select.value=arguments[0];select.dispatchEvent(new Event('change',{bubbles:true}))",theme)
        native.wait_for(lambda:probe()[0]['background']!=previous,f'{theme} checkbox theme')
        client.click("//button[normalize-space()='Done']",'xpath')
        controls=probe()
        assert all(control['width']==16 and control['height']==16 and control['border']=='1px' for control in controls),controls
        assert controls[0]['indicator']=='visible' and controls[1]['indicator']=='hidden',controls
        assert controls[1]['borderColor']!=controls[1]['background'],controls
    for theme in ['dark-plus','darcula','one-dark','monokai','light-plus']:
        change_theme(theme)
    (directory/'checkboxes-light.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
    mark('All six appearance presets retain checkbox sizing, visible borders and checkmarks, including Light+')

    snapshot=client.invoke('sessions')
    session=next(row for row in snapshot['sessions'] if any(tab['id']==diff for tab in row['tabs']))
    created=client.invoke('create_tab',{'sessionId':session['id'],'actionId':'files'})
    files=next(row for row in created['sessions'] if row['id']==session['id'])['selectedTabId']
    client.request('POST','/refresh',{})
    native.wait_for(lambda:client.js("return !!document.querySelector('#tab-panel-'+arguments[0]+' .file-tree button')",files),'Files tree')
    client.click(f"//section[@id='tab-panel-{files}']//button[contains(.,'hello.txt')]",'xpath')
    native.wait_for(lambda:client.js("return !!document.querySelector('.cm-content')"),'Files editor')
    client.click('.editor-toolbar button[popovertarget]')
    client.click("//button[@role='menuitem'][contains(.,'Find / replace')]",'xpath')
    native.wait_for(lambda:client.js("return document.querySelectorAll('.cm-search input[type=checkbox]').length===3"),'search checkboxes')
    controls=client.js("return Array.from(document.querySelectorAll('.cm-search input[type=checkbox]')).map(el=>{const s=getComputedStyle(el),r=el.getBoundingClientRect();return {width:r.width,height:r.height,padding:s.padding,border:s.borderTopWidth,appearance:s.appearance,mark:getComputedStyle(el,'::after').visibility}})")
    assert all(c['width']==16 and c['height']==16 and c['padding']=='0px' and c['border']=='1px' and c['appearance']=='none' and c['mark']=='hidden' for c in controls),controls
    client.fill('.cm-search input[name=search]','CHANGED')
    native.wait_for(lambda:client.js("return document.querySelectorAll('.cm-searchMatch').length>0"),'case-insensitive search')
    client.click('.cm-search input[name=case]')
    native.wait_for(lambda:client.js("return !document.querySelectorAll('.cm-searchMatch').length"),'match-case behavior')
    client.keys(' ')
    native.wait_for(lambda:client.js("return document.querySelectorAll('.cm-searchMatch').length>0"),'keyboard search checkbox behavior')
    (directory/'checkboxes-search.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
    mark('CodeMirror search checkboxes share the same size, border and indicator styling while preserving case-sensitive search and Space-key behavior')
