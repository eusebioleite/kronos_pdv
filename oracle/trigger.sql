create or replace trigger kronos.kronos_pdv after
	insert or update or delete on kronos.f_prgven
	for each row
declare
	v_status char(1);
begin
	select 
		pdv_status
	  into
		v_status
	  from f_pedvenda
	 where pdv_numped = COALESCE(:new.prv_numped, :old.prv_numped);

	if deleting then
		update inventario.kronos_pdv_queue
			set
				status = 'EXCLUIR'
		where order_code = :old.prv_numped 
		and schedule_code = :old.prv_indice;
		return;
	end if;

	if (inserting or updating) and v_status = 'A' then
		merge into inventario.kronos_pdv_queue queue
		using (select :new.prv_numped as order_code, :new.prv_indice as schedule_code from dual) dados
		on (queue.order_code = dados.order_code and queue.schedule_code = dados.schedule_code)
		when matched then
			update set 
				queue.status = 'ATUALIZAR'
		when not matched then
			insert (order_code, schedule_code, status)
			values (dados.order_code, dados.schedule_code, 'NOVO');
	end if;
exception
	when others then
		null;
end;