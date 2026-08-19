create or replace trigger kronos.f_pedvenda_kronos_pdv_queue 
after insert or update or delete on kronos.f_pedvenda
for each row
declare
    v_order_code kronos.f_pedvenda.pdv_numped%type;
begin
    if deleting then
        v_order_code := :old.pdv_numped;
        
        update kronos_pdv_queue queue
           set queue.sync       = 1,
               queue.updated_at = systimestamp
         where queue.order_code = v_order_code;

    elsif :new.pdv_status = 'A' then
        v_order_code := :new.pdv_numped;

        merge into kronos_pdv_queue queue
        using dual
           on (queue.order_code = v_order_code)
        when matched then
            update set 
                queue.sync       = 1,
                queue.updated_at = systimestamp
        when not matched then
            insert (order_code, sync, retries, created_at, updated_at)
            values (v_order_code, 1, 0, systimestamp, systimestamp);
            
    end if;

exception
    when others then
        null;
end;
/